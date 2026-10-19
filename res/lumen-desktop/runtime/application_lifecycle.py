"""Observed native application processes, retained with read-only pidfds.

Application IDs are advertisements, not authenticated application identities.
No process-name search, signalling, automatic quitting, or background launching.
"""
import argparse
import hashlib
import json
import os
import select
import socket
import struct
import subprocess
import sys
from pathlib import Path

from application_control import Backend, window_nodes
from state import runtime_epoch

MAX_PROCESSES = 512
MAX_APP_IDS_PER_PROCESS = 32

class ProcessBackend(Backend):
    def open(self,pid):return os.pidfd_open(pid,0)
    def exited(self,handle):return bool(select.select([handle],[],[],0)[0])

class Lifecycle:
    def __init__(self,backend=None):
        self.backend=backend or ProcessBackend();self.processes={};self.windows={};self.epoch=None
    def close(self):
        for process in self.processes.values():self.backend.close(process['handle'])
        self.processes={};self.epoch=None
    def observe(self,node):
        if not isinstance(node,dict) or node.get('shell')!='xdg_shell' or node.get('window') is not None:return
        app=node.get('app_id');pid=node.get('pid')
        if not isinstance(app,str) or not app or len(app)>256 or app=='org.quickshell' or any(ord(c)<32 for c in app) or type(pid) is not int or pid<2:return
        try:
            identity=self.backend.identity(pid)
            existing=self.processes.get(pid)
            if existing is not None and (self.backend.exited(existing['handle']) or existing['identity']!=identity):
                self.backend.close(existing['handle']);del self.processes[pid];existing=None
            if existing is None:
                if len(self.processes)>=MAX_PROCESSES:return
                handle=self.backend.open(pid)
                try:
                    if self.backend.exited(handle) or self.backend.identity(pid)!=identity:raise ValueError('Process changed during observation')
                except (OSError,ValueError):self.backend.close(handle);raise
                existing={'identity':identity,'handle':handle,'apps':set()};self.processes[pid]=existing
            if len(existing['apps'])<MAX_APP_IDS_PER_PROCESS or app in existing['apps']:existing['apps'].add(app)
        except (OSError,ValueError,IndexError,StopIteration):return
    def refresh(self,epoch,nodes):
        if epoch!=self.epoch:self.close();self.epoch=epoch
        for node in nodes:self.observe(node)
        self.windows={}
        for node in nodes:
            if node.get('shell')=='xdg_shell' and type(node.get('pid')) is int and node.get('app_id'):
                self.windows.setdefault((node['pid'],node['app_id']),[]).append(node.get('id'))
        self.prune()
        return self.rows()
    def prune(self):
        for pid,process in list(self.processes.items()):
            try:live=not self.backend.exited(process['handle']) and self.backend.identity(pid)==process['identity']
            except (OSError,ValueError,IndexError):live=False
            if not live:self.backend.close(process['handle']);del self.processes[pid]
    def rows(self):
        apps={}
        for pid,process in self.processes.items():
            for app in process['apps']:
                windows=self.windows.get((pid,app),[])
                # A browser/PWA process can advertise several application IDs.
                # Process survival cannot establish which zero-window app remains.
                if len(process['apps'])>1 and not windows:continue
                row=apps.setdefault(app,{'appId':app,'windowCount':0,'processCount':0,'instanceIds':[],'sharedProcess':False,'assurance':'native-process-bound-advertised-id'})
                row['instanceIds'].append(hashlib.sha256(json.dumps(process['identity'],sort_keys=True).encode()).hexdigest());row['processCount']+=1;row['windowCount']+=len(windows);row['sharedProcess']|=len(process['apps'])>1
        for row in apps.values():row['zeroWindows']=row['windowCount']==0;row['instanceIds'].sort()
        return sorted(apps.values(),key=lambda row:row['appId'])
    def exact_process(self,app):
        self.prune();matches=[p for p in self.processes.values() if app in p['apps']]
        if len(matches)!=1 or matches[0]['apps']!={app}:raise ValueError('Application ownership is ambiguous or no longer running')
        return matches[0]

class CooperativeApplication:
    def __init__(self,lifecycle,bus=None):
        self.lifecycle=lifecycle
        if bus is None:
            import dbus
            bus=dbus.SessionBus()
        self.bus=bus
    def endpoint(self,app):
        import re
        if not re.fullmatch(r'[A-Za-z_][A-Za-z0-9_]*(?:\.[A-Za-z_][A-Za-z0-9_-]*)+',app):raise ValueError('No supported application protocol identity')
        process=self.lifecycle.exact_process(app)
        owner=str(self.bus.get_name_owner(app))
        daemon=self.bus.get_object('org.freedesktop.DBus','/org/freedesktop/DBus',introspect=False)
        pid=int(daemon.GetConnectionUnixProcessID(owner,dbus_interface='org.freedesktop.DBus',timeout=2))
        if pid!=process['identity']['pid']:raise ValueError('Application protocol owner does not match the observed native process')
        path='/'+app.replace('.','/').replace('-','_')
        return process,owner,self.bus.get_object(owner,path,introspect=False)
    def revalidate(self,app,process,owner):
        current=self.lifecycle.exact_process(app)
        if current is not process or str(self.bus.get_name_owner(app))!=owner:raise ValueError('Application changed before dispatch')
    def quit(self,app):
        process,owner,proxy=self.endpoint(app)
        actions=proxy.List(dbus_interface='org.gtk.Actions',timeout=2)
        if 'quit' not in actions:raise ValueError('This application does not advertise a cooperative Quit action')
        description=proxy.Describe('quit',dbus_interface='org.gtk.Actions',timeout=2)
        if len(description)!=3 or not description[0] or str(description[1]):raise ValueError('The advertised Quit action is unavailable or requires unsupported parameters')
        self.revalidate(app,process,owner)
        import dbus
        proxy.Activate('quit',dbus.Array([],signature='v'),dbus.Dictionary({},signature='sv'),dbus_interface='org.gtk.Actions',timeout=2)
        return {'requested':True,'protocol':'org.gtk.Actions','status':'requested-not-confirmed-exited'}


def receive(stream):
    def read(size):
        data=bytearray()
        while len(data)<size:
            part=stream.recv(size-len(data))
            if not part:raise EOFError('Compositor disconnected')
            data.extend(part)
        return data
    header=read(14)
    if header[:6]!=b'i3-ipc':raise ValueError('Invalid compositor protocol')
    size,kind=struct.unpack('<II',header[6:])
    if size>16*1024*1024:raise ValueError('Compositor response exceeds limit')
    return kind,json.loads(read(size))

def send(stream,kind,value=''):
    data=value.encode();stream.sendall(b'i3-ipc'+struct.pack('<II',len(data),kind)+data)

def watch():
    lifecycle=Lifecycle();last=None
    def emit(available,error='',result=None):
        nonlocal last
        value={'available':available,'applications':lifecycle.rows() if available else [],'error':error}
        if result is not None:value['result']=result
        encoded=json.dumps(value,sort_keys=True)
        if encoded!=last:print(encoded,flush=True);last=encoded
    emit(False)
    try:
        address=os.environ['SWAYSOCK'];epoch=runtime_epoch(address)
        with socket.socket(socket.AF_UNIX) as events,socket.socket(socket.AF_UNIX) as query:
            events.settimeout(5);events.connect(address);send(events,2,'["window","shutdown"]');_,ack=receive(events)
            if not isinstance(ack,dict) or ack.get('success') is not True:raise ValueError('Compositor subscription failed')
            query.settimeout(5);query.connect(address)
            def refresh():
                send(query,4);_,tree=receive(query)
                if runtime_epoch(address)!=epoch:raise ValueError('Compositor session changed')
                lifecycle.refresh(epoch,window_nodes(tree));emit(True)
            refresh();events.settimeout(None)
            while True:
                handles=[p['handle'] for p in lifecycle.processes.values()]
                ready,_,_=select.select([events,sys.stdin]+handles,[],[],1)
                if events in ready:
                    kind,event=receive(events)
                    if kind==0x80000006:raise EOFError('Session ended')
                    if kind==0x80000003 and isinstance(event,dict):lifecycle.observe(event.get('container'))
                    refresh()
                if sys.stdin in ready:
                    line=sys.stdin.readline(65537)
                    if not line:return
                    if len(line)>65536:raise ValueError('Application request exceeds limit')
                    try:
                        data=json.loads(line)
                        if isinstance(data,dict) and data.get('action')=='status':emit(True);continue
                        if not isinstance(data,dict) or data.get('action')!='quit' or not isinstance(data.get('appId'),str):raise ValueError('Unsupported application command')
                        result=CooperativeApplication(lifecycle).quit(data['appId']);result['appId']=data['appId']
                    except Exception:
                        result={'requested':False,'error':'Cooperative Quit is unavailable or the application changed. Use its own Quit command; Force Quit is a separate action.'}
                    emit(True,result=result)
                lifecycle.prune();emit(True)
    except (OSError,ValueError,KeyError,EOFError,TypeError,subprocess.SubprocessError):
        emit(False,'Application lifecycle observation is unavailable; window-based state remains available.')
    finally:lifecycle.close()

def main():
    argparse.ArgumentParser(description=__doc__).parse_args();watch()
if __name__=='__main__':main()

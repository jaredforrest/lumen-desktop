"""Observed native application processes, retained with read-only pidfds.

Application IDs are advertisements, not authenticated application identities.
No process-name search, signalling, automatic quitting, or background launching.
"""
import argparse
import hashlib
import fcntl
import json
import os
import select
import socket
import struct
import stat
import uuid
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

def valid_app_id(app):
    return isinstance(app,str) and 0<len(app)<=256 and app!='org.quickshell' and not any(ord(c)<32 or ord(c)==127 for c in app)


def valid_identity(value):
    return isinstance(value,dict) and set(value)=={'pid','start','device','inode'} and all(type(item) is int and item>=0 for item in value.values()) and value['pid']>=2


class ObservationStore:
    """Private session-local observations; never an application authentication claim."""
    def __init__(self,runtime,epoch,boot_id):
        import re
        if not re.fullmatch('[0-9a-f]{24}',epoch) or not re.fullmatch('[0-9a-f]{8}(?:-[0-9a-f]{4}){3}-[0-9a-f]{12}',boot_id):raise ValueError('Invalid recovery scope')
        root=Path(runtime);info=root.lstat()
        if not stat.S_ISDIR(info.st_mode) or info.st_uid!=os.getuid() or info.st_mode&0o077:raise ValueError('Recovery runtime must be private and owned')
        self.epoch=epoch;self.boot_id=boot_id;self.previous=None;self.warning='';self.directory=None;self.lock=None
        parent=os.open(root,os.O_RDONLY|os.O_DIRECTORY|os.O_NOFOLLOW)
        try:
            opened=os.fstat(parent)
            if (opened.st_dev,opened.st_ino)!=(info.st_dev,info.st_ino) or opened.st_uid!=os.getuid() or opened.st_mode&0o077:raise ValueError('Recovery runtime changed')
            try:os.mkdir('lumen-application-observations',mode=0o700,dir_fd=parent)
            except FileExistsError:pass
            self.directory=os.open('lumen-application-observations',os.O_RDONLY|os.O_DIRECTORY|os.O_NOFOLLOW,dir_fd=parent)
            directory=os.fstat(self.directory)
            if directory.st_uid!=os.getuid() or directory.st_mode&0o077:raise ValueError('Recovery directory must be private and owned')
            self.name=epoch+'.json'
            self.lock=os.open(epoch+'.lock',os.O_CREAT|os.O_RDWR|os.O_NOFOLLOW,0o600,dir_fd=self.directory)
            lock=os.fstat(self.lock)
            if not stat.S_ISREG(lock.st_mode) or lock.st_uid!=os.getuid() or lock.st_mode&0o077 or lock.st_nlink!=1:raise ValueError('Invalid recovery lease')
            fcntl.flock(self.lock,fcntl.LOCK_EX|fcntl.LOCK_NB)
        except BaseException:
            self.close();raise
        finally:os.close(parent)
    def close(self):
        if self.lock is not None:os.close(self.lock);self.lock=None
        if self.directory is not None:os.close(self.directory);self.directory=None
    def load(self):
        try:
            fd=os.open(self.name,os.O_RDONLY|os.O_NOFOLLOW|os.O_NONBLOCK,dir_fd=self.directory)
            with os.fdopen(fd,'rb') as stream:
                info=os.fstat(stream.fileno())
                if not stat.S_ISREG(info.st_mode) or info.st_uid!=os.getuid() or info.st_mode&0o077 or info.st_nlink!=1 or info.st_size>1048576:raise ValueError('Invalid recovery record')
                raw=stream.read(1048577)
            if len(raw)>1048576:raise ValueError('Recovery record exceeds limit')
            data=json.loads(raw)
            if not isinstance(data,dict) or set(data)!={'version','epoch','bootId','processes'} or type(data['version']) is not int or data['version']!=1 or data['epoch']!=self.epoch or data['bootId']!=self.boot_id:raise ValueError('Recovery scope changed')
            rows=data['processes']
            if not isinstance(rows,list) or len(rows)>MAX_PROCESSES:raise ValueError('Invalid recovery process list')
            pids=set()
            for row in rows:
                if not isinstance(row,dict) or set(row)!={'identity','apps'} or not valid_identity(row['identity']) or row['identity']['pid'] in pids:raise ValueError('Invalid recovery identity')
                apps=row['apps']
                if not isinstance(apps,list) or not 1<=len(apps)<=MAX_APP_IDS_PER_PROCESS or not all(valid_app_id(app) for app in apps) or len(set(apps))!=len(apps):raise ValueError('Invalid recovered advertisement')
                pids.add(row['identity']['pid'])
            return rows
        except FileNotFoundError:return []
        except (OSError,ValueError,KeyError,TypeError,RecursionError):
            self.warning='Previous application observations were not restored.';return []
    def invalidate(self):
        try:os.unlink(self.name,dir_fd=self.directory)
        except FileNotFoundError:pass
        self.previous=None
        os.fsync(self.directory)
    def save(self,rows):
        data={'version':1,'epoch':self.epoch,'bootId':self.boot_id,'processes':rows}
        encoded=json.dumps(data,sort_keys=True,separators=(',',':')).encode()
        if encoded==self.previous:return
        if len(encoded)>1048576:raise ValueError('Recovery record exceeds limit')
        name='.'+self.name+'.'+uuid.uuid4().hex
        fd=os.open(name,os.O_WRONLY|os.O_CREAT|os.O_EXCL|os.O_NOFOLLOW,0o600,dir_fd=self.directory)
        try:
            with os.fdopen(fd,'wb') as stream:stream.write(encoded);stream.flush();os.fsync(stream.fileno())
            os.replace(name,self.name,src_dir_fd=self.directory,dst_dir_fd=self.directory);os.fsync(self.directory);self.previous=encoded
        finally:
            try:os.unlink(name,dir_fd=self.directory)
            except FileNotFoundError:pass


class Lifecycle:
    def __init__(self,backend=None):
        self.backend=backend or ProcessBackend();self.processes={};self.windows={};self.epoch=None
    def close(self):
        for process in self.processes.values():self.backend.close(process['handle'])
        self.processes={};self.epoch=None
    def observe(self,node):
        if not isinstance(node,dict) or node.get('shell')!='xdg_shell' or node.get('window') is not None:return
        app=node.get('app_id');pid=node.get('pid')
        if not valid_app_id(app) or type(pid) is not int or pid<2:return
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
    def recover(self,epoch,rows):
        if epoch!=self.epoch:self.close();self.epoch=epoch
        for row in rows:
            identity=row['identity'];pid=identity['pid'];handle=None
            try:
                if self.backend.identity(pid)!=identity:continue
                handle=self.backend.open(pid)
                if self.backend.exited(handle) or self.backend.identity(pid)!=identity:raise ValueError('Recovered process changed')
                if pid in self.processes:self.backend.close(self.processes[pid]['handle'])
                self.processes[pid]={'identity':identity.copy(),'handle':handle,'apps':set(row['apps'])};handle=None
            except (OSError,ValueError,IndexError):pass
            finally:
                if handle is not None:self.backend.close(handle)
        self.prune()
    def records(self):
        return [{'identity':value['identity'].copy(),'apps':sorted(value['apps'])} for _,value in sorted(self.processes.items())]
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
    def prepare_quit(self,app):
        process,owner,proxy=self.endpoint(app)
        actions=proxy.List(dbus_interface='org.gtk.Actions',timeout=2)
        if 'quit' not in actions:raise ValueError('This application does not advertise a cooperative Quit action')
        description=proxy.Describe('quit',dbus_interface='org.gtk.Actions',timeout=2)
        if len(description)!=3 or not description[0] or str(description[1]):raise ValueError('The advertised Quit action is unavailable or requires unsupported parameters')
        self.revalidate(app,process,owner)
        return (app,process,owner,proxy)
    def dispatch_quit(self,prepared):
        app,process,owner,proxy=prepared
        self.revalidate(app,process,owner)
        import dbus
        proxy.Activate('quit',dbus.Array([],signature='v'),dbus.Dictionary({},signature='sv'),dbus_interface='org.gtk.Actions',timeout=2)
        return {'requested':True,'protocol':'org.gtk.Actions','status':'requested-not-confirmed-exited'}
    def quit(self,app):return self.dispatch_quit(self.prepare_quit(app))


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
    from session_end import SessionEnd
    lifecycle=Lifecycle();last=None;store=None;nodes=[]
    session=SessionEnd(lifecycle,CooperativeApplication(lifecycle),shell_pid=os.getppid())
    def emit(available,error='',result=None):
        nonlocal last
        recovery=False
        if available and store is not None:
            try:store.save(lifecycle.records());recovery=True
            except (OSError,ValueError):
                try:store.invalidate()
                except OSError:pass
                error='Application observations could not be saved for shell recovery.'
        value={'available':available,'applications':lifecycle.rows() if available else [],'error':error,'recoveryAvailable':recovery,'session':session.state()}
        if result is not None:value['result']=result
        encoded=json.dumps(value,sort_keys=True)
        if encoded!=last:print(encoded,flush=True);last=encoded
    emit(False)
    try:
        address=os.environ['SWAYSOCK'];epoch=runtime_epoch(address)
        boot_id=Path('/proc/sys/kernel/random/boot_id').read_text().strip()
        store=ObservationStore(os.environ['XDG_RUNTIME_DIR'],epoch,boot_id)
        with socket.socket(socket.AF_UNIX) as events,socket.socket(socket.AF_UNIX) as query:
            events.settimeout(5);events.connect(address);send(events,2,'["window","shutdown"]');_,ack=receive(events)
            if not isinstance(ack,dict) or ack.get('success') is not True:raise ValueError('Compositor subscription failed')
            query.settimeout(5);query.connect(address)
            def refresh():
                nonlocal nodes
                send(query,4);_,tree=receive(query)
                if runtime_epoch(address)!=epoch:raise ValueError('Compositor session changed')
                nodes=window_nodes(tree);lifecycle.refresh(epoch,nodes);session.tick(nodes);emit(True)
            lifecycle.recover(epoch,store.load());refresh();events.settimeout(None);input_buffer=b''
            while True:
                handles=[p['handle'] for p in lifecycle.processes.values()]
                ready,_,_=select.select([events,sys.stdin]+handles,[],[],1)
                if events in ready:
                    kind,event=receive(events)
                    if kind==0x80000006:store.save([]);raise EOFError('Session ended')
                    if kind==0x80000003 and isinstance(event,dict):lifecycle.observe(event.get('container'))
                    refresh()
                if sys.stdin in ready:
                    chunk=os.read(sys.stdin.fileno(),65537)
                    if not chunk:return
                    input_buffer+=chunk
                    if len(input_buffer)>65536 and b'\n' not in input_buffer:raise ValueError('Application request exceeds limit')
                    lines=input_buffer.split(b'\n');input_buffer=lines.pop()
                    for line in lines:
                        if len(line)>65536:raise ValueError('Application request exceeds limit')
                        data=None
                        try:
                            data=json.loads(line)
                            if isinstance(data,dict) and data.get('action')=='status':emit(True);continue
                            if isinstance(data,dict) and data.get('action') in ('session_begin','session_cancel','session_authorize'):
                                command=data['action']
                                if command=='session_cancel':session.cancel();emit(True);continue
                                refresh()
                                if command=='session_begin':session.begin(data.get('sessionAction'),nodes);emit(True);continue
                                authorization=session.authorize(data.get('token'),nodes);emit(True,result={'sessionAuthorization':authorization});continue
                            if not isinstance(data,dict) or data.get('action')!='quit' or not isinstance(data.get('appId'),str):raise ValueError('Unsupported application command')
                            result=CooperativeApplication(lifecycle).quit(data['appId']);result['appId']=data['appId']
                        except Exception:
                            if isinstance(data,dict) and str(data.get('action','')).startswith('session_'):session.fail('Session confirmation failed. The session remains open; cancel and start again.')
                            result={'requested':False,'error':'Cooperative Quit is unavailable or the application changed. Use its own Quit command; Force Quit is a separate action.'}
                        emit(True,result=result)
                lifecycle.prune();session.tick(nodes);emit(True)
    except (OSError,ValueError,KeyError,EOFError,TypeError,subprocess.SubprocessError):
        emit(False,'Application lifecycle observation is unavailable; window-based state remains available.')
    finally:
        session.cancel();lifecycle.close()
        if store is not None:store.close()

def main():
    argparse.ArgumentParser(description=__doc__).parse_args();watch()
if __name__=='__main__':main()

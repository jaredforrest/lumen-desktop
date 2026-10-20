"""Native lock authority. Wakeups carry no authority; every publication rechecks IPC."""
import json
import os
import socket
import stat
import struct
import time

MAGIC=b'i3-ipc'
HEADER=struct.Struct('<6sII')
LIMIT=65536

class SessionLockAuthority:
    def __init__(self, glib=None, changed=None, path=None):
        self.glib,self.changed=glib,changed
        self.path=path if path is not None else os.environ.get('SWAYSOCK','')
        self.monitor=None;self.watch=None;self.token=None;self.last_token=None;self.revision=-1
        self.buffer=bytearray();self.retry=0
        self.facts={'known':False,'locked':True}

    def _socket(self):
        info=os.stat(self.path)
        if not stat.S_ISSOCK(info.st_mode):raise ValueError('Not an IPC socket')
        connection=socket.socket(socket.AF_UNIX,socket.SOCK_STREAM)
        try:
            connection.settimeout(.2);connection.connect(self.path)
            pid,uid,gid=struct.unpack('iII',connection.getsockopt(socket.SOL_SOCKET,socket.SO_PEERCRED,12))
            current=os.stat(self.path)
            if (current.st_dev,current.st_ino)!=(info.st_dev,info.st_ino):raise ValueError('Changed IPC socket')
            if uid!=os.geteuid():raise ValueError('Different IPC owner')
            return connection,(pid,uid,gid,info.st_dev,info.st_ino)
        except Exception:
            connection.close();raise

    @staticmethod
    def _send(connection,kind,value):
        body=json.dumps(value).encode();connection.sendall(HEADER.pack(MAGIC,len(body),kind)+body)

    @staticmethod
    def _receive(connection,kind):
        def exact(size):
            result=bytearray()
            while len(result)<size:
                chunk=connection.recv(size-len(result))
                if not chunk:raise ValueError('Closed IPC')
                result.extend(chunk)
            return result
        magic,size,actual=HEADER.unpack(exact(HEADER.size))
        if magic!=MAGIC or size>LIMIT or actual!=kind:raise ValueError('Invalid IPC frame')
        return json.loads(exact(size))

    def _unknown(self):
        self.facts={'known':False,'locked':True}

    def close(self):
        if self.watch is not None and self.glib is not None:self.glib.source_remove(self.watch)
        self.watch=None
        if self.monitor is not None:self.monitor.close()
        self.monitor=None;self.token=None;self.buffer.clear();self._unknown()

    def ensure_monitor(self):
        if self.monitor is not None:return
        if time.monotonic()<self.retry:return
        self.retry=time.monotonic()+1
        try:
            connection,token=self._socket()
            try:
                self._send(connection,2,['tick','shutdown'])
                reply=self._receive(connection,2)
                if not isinstance(reply,dict) or reply.get('success') is not True:raise ValueError('Subscription rejected')
            except Exception:
                connection.close();raise
            connection.setblocking(False);self.monitor,self.token=connection,token
            if token!=self.last_token:self.revision=-1
            self.last_token=token
            if self.glib is not None:
                self.watch=self.glib.io_add_watch(connection,self.glib.PRIORITY_DEFAULT,self.glib.IO_IN|self.glib.IO_HUP|self.glib.IO_ERR,self._events,connection)
        except (OSError,ValueError,TypeError,UnicodeError):self._unknown()

    def refresh(self):
        before=dict(self.facts);self.ensure_monitor()
        if self.monitor is None:self._unknown();return before!=self.facts
        try:
            # Detect a dead event channel before accepting an unlocked query.
            try:
                if self.monitor.recv(1,socket.MSG_PEEK)==b'':raise ValueError('Closed event channel')
            except BlockingIOError:pass
            connection,token=self._socket()
            try:
                if token!=self.token:raise ValueError('Changed compositor')
                self._send(connection,7,'')
                reply=self._receive(connection,7)
                if not isinstance(reply,dict):raise ValueError('Invalid native lock reply')
                value=reply.get('lumen_session_lock')
            finally:connection.close()
            if not isinstance(value,dict) or type(value.get('version')) is not int or value['version']!=1 or value.get('available') is not True or type(value.get('locked')) is not bool or type(value.get('revision')) is not int or not 0<=value['revision']<=2**63-1 or value['revision']<self.revision:
                raise ValueError('Unknown lock authority')
            self.revision=value['revision'];self.facts={'known':True,'locked':value['locked']}
        except (OSError,ValueError,TypeError,UnicodeError):
            self.close()
        return before!=self.facts

    def snapshot(self):
        self.refresh();return dict(self.facts)

    def _events(self,source,condition,connection):
        # A queued callback from an old subscription can never alter current facts.
        if connection is not self.monitor:return False
        before=dict(self.facts)
        try:
            if condition&(self.glib.IO_HUP|self.glib.IO_ERR):raise ValueError('Disconnected authority')
            chunk=connection.recv(LIMIT)
            if not chunk:raise ValueError('Closed authority')
            self.buffer.extend(chunk)
            wake=False
            while len(self.buffer)>=HEADER.size:
                magic,size,kind=HEADER.unpack(self.buffer[:HEADER.size])
                if magic!=MAGIC or size>LIMIT:raise ValueError('Invalid authority event')
                if len(self.buffer)<HEADER.size+size:break
                del self.buffer[:HEADER.size+size]
                if kind==0x80000006:raise ValueError('Compositor shutdown')
                wake=True
            if wake:self.refresh()
        except (OSError,ValueError,TypeError):
            # The current GLib source removes itself on False, not from inside close().
            self.watch=None;self.close()
        if before!=self.facts and self.changed is not None:self.changed()
        return connection is self.monitor


def previews_allowed(preview,facts,host_locked=False):
    if preview=='never' or facts.get('known') is not True:return False
    if facts.get('locked') is False:return preview in ('always','when_unlocked')
    return preview=='always' and host_locked is True

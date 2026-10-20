"""Asynchronous network locations with toolkit-owned authentication.

Only credential-free addresses and mount state cross the JSON interface.
Passwords, keyring choices and server questions stay inside Gtk.MountOperation.
No connection history or credentials are written to disk.
"""
import json
import os
import re
import sys
import uuid
from urllib.parse import quote,urlsplit,urlunsplit
from gi.repository import Gio,GLib

class RequestError(ValueError):
    pass

PROTOCOLS=('smb','sftp','dav','davs','ftp','ftps','afp','nfs')
MAX_URI=4096
MAX_ROWS=100
ATTRIBUTES='standard::name,standard::display-name,standard::type,standard::size,time::modified'

def address(value,strip_identity=False):
    try:encoded=value.encode('utf-8') if isinstance(value,str) else b''
    except UnicodeError:raise RequestError('Enter a valid server address') from None
    if not isinstance(value,str) or not value or len(encoded)>MAX_URI or any(ord(char)<32 for char in value):
        raise RequestError('Enter a server address of at most 4096 bytes')
    try:parsed=urlsplit(value)
    except ValueError:raise RequestError('Enter a valid server address') from None
    if parsed.scheme.lower() not in PROTOCOLS:raise RequestError('Choose a supported network protocol')
    if not strip_identity and (parsed.username is not None or parsed.password is not None):
        raise RequestError('Do not include a username or password in the address; the system dialog will ask when needed')
    if parsed.query or parsed.fragment:raise RequestError('Server addresses cannot contain a query or fragment')
    host=parsed.hostname
    if not host or '%' in host or '\\' in host or any(char.isspace() for char in host):raise RequestError('Enter a valid server hostname')
    try:
        port=parsed.port
        if port is not None and not 1<=port<=65535:raise RequestError()
        if ':' in host:
            import ipaddress
            host='['+str(ipaddress.IPv6Address(host))+']'
        else:
            host=host.encode('idna').decode('ascii').lower()
            if not re.fullmatch(r'[a-z0-9_.-]+',host):raise RequestError()
    except (ValueError,UnicodeError):raise RequestError('Enter a valid hostname and port') from None
    path=parsed.path or '/'
    if re.search(r'%(?![0-9A-Fa-f]{2})',path):raise RequestError('The server path has an invalid escape')
    path=quote(path,safe="/:@!$&'()*+,;=-._~%")
    return urlunsplit((parsed.scheme.lower(),host+(':'+str(port) if port else ''),path,'',''))

def toolkit_operation():
    import gi
    gi.require_version('Gtk','3.0')
    from gi.repository import Gtk
    result=Gtk.init_check()
    if not (result[0] if isinstance(result,tuple) else result):raise RequestError('A graphical session is required for network authentication')
    # Gtk supplies the password and keyring controls. Never inspect their values.
    return Gtk.MountOperation.new(None)

def failure(error):
    # Provider messages may echo addresses or credentials. Only stable categories
    # leave this process, including for unexpected provider exceptions.
    if isinstance(error,GLib.Error):
        for code,text in (
            (Gio.IOErrorEnum.CANCELLED,'The operation was cancelled'),
            (Gio.IOErrorEnum.NOT_SUPPORTED,'This network provider is not available in the current session'),
            (Gio.IOErrorEnum.NOT_MOUNTED,'The network location is disconnected'),
            (Gio.IOErrorEnum.HOST_NOT_FOUND,'The server could not be found'),
            (Gio.IOErrorEnum.CONNECTION_REFUSED,'The server refused the connection'),
            (Gio.IOErrorEnum.PERMISSION_DENIED,'The server denied access; check the account in the system dialog'),
            (Gio.IOErrorEnum.TIMED_OUT,'The network request timed out'),
            (Gio.IOErrorEnum.NOT_FOUND,'This network folder is no longer available'),
        ):
            if error.matches(Gio.io_error_quark(),code):return text
    return 'The network provider could not complete the operation'

class Backend:
    def __init__(self,emit,*,monitor=None,vfs=None,file_factory=None,operation_factory=None,schedule=None,remove_timer=None):
        self.emit=emit
        self.monitor=monitor if monitor is not None else Gio.VolumeMonitor.get()
        self.vfs=vfs if vfs is not None else Gio.Vfs.get_default()
        self.file_factory=file_factory or Gio.File.new_for_uri
        self.operation_factory=operation_factory or toolkit_operation
        self.schedule=schedule or GLib.timeout_add_seconds
        self.remove_timer=remove_timer or GLib.source_remove
        self.connections={}
        self.operations={}
        self.closed=False
        for signal in ('mount-added','mount-removed','mount-changed'):
            self.monitor.connect(signal,lambda *unused:self.refresh_mounts())
        self.refresh_mounts(notify=False)

    def protocols(self):
        available=set(self.vfs.get_supported_uri_schemes())
        return [scheme for scheme in PROTOCOLS if scheme in available]

    def public(self,connection):
        return {key:connection.get(key) for key in ('id','name','uri','state','path','canDisconnect','error')}

    def snapshot(self):
        protocols=self.protocols()
        return {'connections':[self.public(value) for value in self.connections.values()],
                'protocols':protocols,'available':bool(protocols),
                'reason':'' if protocols else 'Network providers are unavailable; enable the optional GVfs host integration'}

    def changed(self):
        if not self.closed:self.emit({'event':'state',**self.snapshot()})

    def refresh_mounts(self,notify=True):
        present=[]
        for mount in self.monitor.get_mounts():
            try:
                uri=address(mount.get_root().get_uri(),strip_identity=True)
            except (ValueError,AttributeError):continue
            present.append(mount)
            root=self.file_factory(uri)
            existing=next((entry for entry in self.connections.values() if entry.get('mount')==mount),None)
            if existing is None:
                existing=next((entry for entry in self.connections.values() if entry.get('state') in ('connecting','authenticating','disconnected') and (entry['root'].equal(root) or entry['root'].has_prefix(root))),None)
            if existing is None:
                ident=uuid.uuid4().hex
                existing=dict(id=ident,uri=uri,name=urlsplit(uri).hostname,root=root,generation=0)
                self.connections[ident]=existing
            existing['mount']=mount
            if existing.get('state') not in ('connecting','authenticating','disconnecting'):
                existing.update(state='connected',mount=mount,path=existing['root'].get_path() or '',canDisconnect=bool(mount.can_unmount()),error='')
        for entry in self.connections.values():
            if entry.get('mount') is not None and entry['mount'] not in present and entry.get('state')!='disconnecting':
                entry.update(state='disconnected',mount=None,path='',canDisconnect=False,error='The network location disconnected')
                entry['generation']+=1
                for ident,operation in list(self.operations.items()):
                    if operation['connection']==entry['id']:self.cancel(ident,'The network location disconnected')
        if notify:self.changed()

    def result(self,ident,action,**values):self.emit(dict(id=ident,action=action,**values))

    def dispatch(self,request):
        if not isinstance(request,dict):raise RequestError('A network request is required')
        ident=request.get('id')
        if type(ident) is not int or not 1<=ident<=2**53-1:raise RequestError('A positive request ID is required')
        action=request.get('action')
        fields={'status':set(),'connect':{'uri'},'reconnect':{'connectionId'},'disconnect':{'connectionId'},'browse':{'connectionId','uri'},'cancel':{'target'}}
        if action not in fields or set(request)-({'id','action'}|fields[action]):raise RequestError('Unsupported network request fields')
        if action in ('reconnect','disconnect','browse') and (not isinstance(request.get('connectionId'),str) or not re.fullmatch(r'[0-9a-f]{32}',request['connectionId'])):raise RequestError('Choose a known network location')
        if action=='status':self.refresh_mounts(notify=False);self.result(ident,action,**self.snapshot());return
        if action=='cancel':
            target=request.get('target')
            if type(target) is not int:raise RequestError('Choose an active request to cancel')
            self.cancel(target);self.result(ident,action,cancelled=target);return
        if ident in self.operations:raise RequestError('This network request ID is already active')
        if len(self.operations)>=8:raise RequestError('Wait for a pending network operation or cancel it')
        if action in ('connect','reconnect'):
            entry=self.connections.get(request.get('connectionId')) if action=='reconnect' else None
            if action=='reconnect' and entry is None:raise RequestError('This network location is no longer known')
            uri=entry['uri'] if entry else address(request.get('uri'))
            if urlsplit(uri).scheme not in self.protocols():raise RequestError('This network protocol is unavailable; enable its GVfs provider in the host configuration')
            if entry and entry.get('state') in ('connecting','authenticating','disconnecting'):raise RequestError('This network location is busy')
            self.connect(ident,uri,entry,action);return
        entry=self.connections.get(request.get('connectionId'))
        if entry is None:raise RequestError('This network location is no longer known')
        if action=='disconnect':self.disconnect(ident,entry);return
        if action=='browse':self.browse(ident,entry,request.get('uri'));return
        raise RequestError('Unsupported network action')

    def begin(self,ident,action,entry,seconds,*,toolkit=False):
        operation=dict(action=action,connection=entry['id'],generation=entry['generation'],cancellable=Gio.Cancellable(),toolkit=None,enumerator=None)
        if toolkit:
            operation['toolkit']=self.operation_factory()
            for signal in ('ask-password','ask-question'):
                operation['toolkit'].connect(signal,lambda *unused:self.authenticating(ident))
        self.operations[ident]=operation
        operation['timer']=self.schedule(seconds,lambda:self.timeout(ident))
        return operation

    def alive(self,ident):
        operation=self.operations.get(ident)
        if not operation:return None
        entry=self.connections.get(operation['connection'])
        return operation if entry and entry['generation']==operation['generation'] else None

    def authenticating(self,ident):
        operation=self.alive(ident)
        if operation:
            entry=self.connections[operation['connection']];entry['state']='authenticating';self.changed()

    def release(self,ident):
        operation=self.operations.pop(ident,None)
        if operation:
            self.remove_timer(operation['timer'])
            enumerator=operation.get('enumerator')
            if enumerator:
                # Closing the local handle is asynchronous too: a dead network
                # provider must never block the UI's event loop on close.
                try:enumerator.close_async(GLib.PRIORITY_DEFAULT,None,None,None)
                except Exception:pass
        return operation

    def complete(self,ident,error=None,**values):
        operation=self.alive(ident)
        if not operation:return
        entry=self.connections[operation['connection']]
        action=operation['action']
        self.release(ident)
        if error:
            if action in ('connect','reconnect','disconnect'):
                entry.update(state='disconnected' if action!='disconnect' else 'connected',error=failure(error),path='' if action!='disconnect' else entry.get('path',''))
            self.result(ident,action,error=failure(error),connectionId=entry['id'])
        else:self.result(ident,action,connectionId=entry['id'],**values)
        self.changed()

    def cancel(self,ident,message='The operation was cancelled'):
        operation=self.release(ident)
        if not operation:return
        operation['cancellable'].cancel()
        if operation['toolkit']:
            # Tell Gtk to dismiss any authentication dialog. Do not read, copy
            # or manipulate its credential fields.
            operation['toolkit'].emit('aborted')
        entry=self.connections.get(operation['connection'])
        if entry and operation['action'] in ('connect','reconnect','disconnect'):
            entry.update(state='disconnected' if operation['action']!='disconnect' else 'connected',error=message)
            if operation['action']!='disconnect':entry['path']=''
        self.result(ident,operation['action'],connectionId=operation['connection'],error=message,cancelled=True)
        self.changed()

    def timeout(self,ident):self.cancel(ident,'The network request timed out; retry or check the connection');return False

    def connect(self,ident,uri,entry=None,action='connect'):
        if entry is None:
            entry=dict(id=uuid.uuid4().hex,uri=uri,name=urlsplit(uri).hostname,state='disconnected',path='',mount=None,canDisconnect=False,error='',root=self.file_factory(uri),generation=0)
            self.connections[entry['id']]=entry
        entry['generation']+=1
        operation=self.begin(ident,action,entry,120,toolkit=True)
        entry.update(state='connecting',error='',path='',canDisconnect=False)
        self.changed()
        target=entry['root']
        def mounted(file,result,*unused):
            if not self.alive(ident):return
            try:file.mount_enclosing_volume_finish(result)
            except GLib.Error as error:
                if not error.matches(Gio.io_error_quark(),Gio.IOErrorEnum.ALREADY_MOUNTED):self.complete(ident,error);return
            try:target.find_enclosing_mount_async(GLib.PRIORITY_DEFAULT,operation['cancellable'],located,None)
            except Exception as error:self.complete(ident,error)
        def located(file,result,*unused):
            if not self.alive(ident):return
            try:
                mount=file.find_enclosing_mount_finish(result)
                entry.update(mount=mount,state='connected',path=target.get_path() or '',canDisconnect=bool(mount.can_unmount()),error='')
                self.complete(ident,connected=True,path=entry['path'],uri=entry['uri'])
            except Exception as error:self.complete(ident,error)
        try:target.mount_enclosing_volume(Gio.MountMountFlags.NONE,operation['toolkit'],operation['cancellable'],mounted,None)
        except Exception as error:self.complete(ident,error)

    def disconnect(self,ident,entry):
        mount=entry.get('mount')
        if entry.get('state')!='connected' or not mount or not mount.can_unmount():raise RequestError('This network location cannot be disconnected')
        operation=self.begin(ident,'disconnect',entry,30,toolkit=True)
        entry.update(state='disconnecting',error='');self.changed()
        def done(mount,result,*unused):
            if not self.alive(ident):return
            try:
                mount.unmount_with_operation_finish(result)
                entry.update(state='disconnected',mount=None,path='',canDisconnect=False,error='');entry['generation']+=1
                operation['generation']=entry['generation']
                self.complete(ident,disconnected=True)
            except Exception as error:self.complete(ident,error)
        try:mount.unmount_with_operation(Gio.MountUnmountFlags.NONE,operation['toolkit'],operation['cancellable'],done,None)
        except Exception as error:self.complete(ident,error)

    def browse(self,ident,entry,uri=None):
        if entry.get('state')!='connected':raise RequestError('Connect this network location before browsing')
        target=self.file_factory(address(uri or entry['uri']))
        if not target.equal(entry['root']) and not target.has_prefix(entry['root']):raise RequestError('Choose a folder inside this network location')
        # A newer navigation supersedes the previous browse, without disconnecting.
        for old,operation in list(self.operations.items()):
            if operation['connection']==entry['id'] and operation['action']=='browse':self.cancel(old,'A newer folder request replaced this request')
        operation=self.begin(ident,'browse',entry,8)
        operation.update(rows=[],target=target,scanned=0)
        def enumerated(file,result,*unused):
            if not self.alive(ident):return
            try:
                operation['enumerator']=file.enumerate_children_finish(result)
                operation['enumerator'].next_files_async(32,GLib.PRIORITY_DEFAULT,operation['cancellable'],batch,None)
            except Exception as error:self.complete(ident,error)
        def batch(enumerator,result,*unused):
            if not self.alive(ident):return
            try:
                infos=enumerator.next_files_finish(result)
                for info in infos:
                    operation["scanned"]+=1
                    if operation["scanned"]>1000:break
                    name=info.get_name()
                    if not isinstance(name,str) or name in ('','.','..') or '/' in name or '\0' in name:continue
                    child=target.get_child(name)
                    try:child_uri=address(child.get_uri(),strip_identity=True)
                    except ValueError:continue
                    local=child.get_path() or ''
                    if len(local.encode('utf-8'))>MAX_URI:local=''
                    operation['rows'].append(dict(name=(info.get_display_name() or name)[:256],uri=child_uri,path=local,directory=info.get_file_type()==Gio.FileType.DIRECTORY,size=info.get_size(),modified=info.get_attribute_uint64('time::modified')))
                    if len(operation['rows'])>MAX_ROWS:break
                if infos and len(operation['rows'])<=MAX_ROWS and operation['scanned']<=1000:
                    enumerator.next_files_async(32,GLib.PRIORITY_DEFAULT,operation['cancellable'],batch,None);return
                rows=operation['rows'];rows.sort(key=lambda row:(not row['directory'],row['name'].casefold()))
                parent=target.get_parent();parent_uri=address(parent.get_uri(),strip_identity=True) if parent and (parent.equal(entry['root']) or parent.has_prefix(entry['root'])) else ''
                self.complete(ident,rows=rows[:MAX_ROWS],limited=len(rows)>MAX_ROWS or operation['scanned']>1000,uri=address(target.get_uri(),strip_identity=True),parent=parent_uri,path=target.get_path() or '')
            except Exception as error:self.complete(ident,error)
        try:target.enumerate_children_async(ATTRIBUTES,Gio.FileQueryInfoFlags.NOFOLLOW_SYMLINKS,GLib.PRIORITY_DEFAULT,operation['cancellable'],enumerated,None)
        except Exception as error:self.complete(ident,error)

    def close(self):
        self.closed=True
        for ident in list(self.operations):self.cancel(ident)

def main():
    import fcntl
    os.umask(0o077)
    def emit(value):print(json.dumps(value,ensure_ascii=True,separators=(',',':')),flush=True)
    backend=Backend(emit)
    loop=GLib.MainLoop();buffer=bytearray()
    flags=fcntl.fcntl(sys.stdin.fileno(),fcntl.F_GETFL);fcntl.fcntl(sys.stdin.fileno(),fcntl.F_SETFL,flags|os.O_NONBLOCK)
    def readable(source,condition):
        try:chunk=os.read(sys.stdin.fileno(),65536)
        except BlockingIOError:return True
        if not chunk:backend.close();loop.quit();return False
        buffer.extend(chunk)
        if len(buffer)>65536 and b'\n' not in buffer:backend.close();loop.quit();return False
        while b'\n' in buffer:
            line,_,rest=buffer.partition(b'\n');buffer[:]=rest
            if len(line)>65536:emit({'error':'Network request exceeds its size limit'});continue
            request=None
            try:
                request=json.loads(line)
                backend.dispatch(request)
            except (ValueError,TypeError) as error:
                # Only our fixed validation strings may reach the transport.
                emit({'id':request.get('id') if isinstance(request,dict) else None,'error':str(error) if isinstance(error,RequestError) else 'Invalid network request'})
            except Exception:emit({'id':request.get('id') if isinstance(request,dict) else None,'error':'The network request could not be processed'})
        return True
    GLib.io_add_watch(sys.stdin.fileno(), GLib.PRIORITY_DEFAULT, GLib.IO_IN|GLib.IO_HUP,readable)
    emit({'event':'ready',**backend.snapshot()})
    try:loop.run()
    finally:backend.close()
if __name__=='__main__':main()

"""Private fake providers prove state, cancellation and credential boundaries."""
import sys,unittest
from pathlib import Path
from urllib.parse import urlsplit
sys.path.insert(0,str(Path(__file__).resolve().parents[2]/'res/lumen-desktop/shell'))
import network_locations as network
from gi.repository import Gio,GLib
class Operation:
    def __init__(self):self.signals={};self.emitted=[]
    def connect(self,signal,callback):self.signals[signal]=callback
    def emit(self,signal,*args):
        self.emitted.append(signal)
        if signal in self.signals:self.signals[signal](self,*args)
class Monitor:
    def __init__(self):self.mounts=[]
    def connect(self,*args):pass
    def get_mounts(self):return self.mounts
class Vfs:
    def get_supported_uri_schemes(self):return ['file','sftp','smb']
class Info:
    def __init__(self,name,directory=False):self.name=name;self.directory=directory
    def get_name(self):return self.name
    def get_display_name(self):return self.name
    def get_file_type(self):return Gio.FileType.DIRECTORY if self.directory else Gio.FileType.REGULAR
    def get_size(self):return 7
    def get_attribute_uint64(self,key):return 1
class Enumerator:
    def __init__(self,infos):self.infos=infos;self.closed=False
    def next_files_async(self,size,priority,cancellable,callback,data):
        batch,self.infos=self.infos[:size],self.infos[size:];callback(self,batch,data)
    def next_files_finish(self,result):return result
    def close_async(self,*args):self.closed=True
class File:
    def __init__(self,provider,uri):self.provider=provider;self.uri=uri
    def get_uri(self):return self.uri
    def get_path(self):return self.provider.paths.get(self.uri)
    def equal(self,other):return self.uri==other.uri
    def has_prefix(self,other):return self.uri.startswith(other.uri.rstrip('/')+'/')
    def get_child(self,name):return self.provider.file(self.uri.rstrip('/')+'/'+name)
    def get_parent(self):
        parsed=urlsplit(self.uri);path=parsed.path.rstrip('/');parent=path.rsplit('/',1)[0] or '/'
        if parent==parsed.path:return None
        return self.provider.file(parsed.scheme+'://'+parsed.netloc+parent)
    def mount_enclosing_volume(self,flags,operation,cancellable,callback,data):self.provider.mount_requests.append((self,operation,cancellable,callback,data))
    def mount_enclosing_volume_finish(self,result):
        if isinstance(result,Exception):raise result
        return True
    def find_enclosing_mount_async(self,priority,cancellable,callback,data):self.provider.locate_requests.append((self,cancellable,callback,data))
    def find_enclosing_mount_finish(self,result):
        if isinstance(result,Exception):raise result
        return result
    def enumerate_children_async(self,attrs,flags,priority,cancellable,callback,data):self.provider.browse_requests.append((self,cancellable,callback,data))
    def enumerate_children_finish(self,result):
        if isinstance(result,Exception):raise result
        return result
class Provider:
    def __init__(self):self.mount_requests=[];self.locate_requests=[];self.browse_requests=[];self.paths={}
    def file(self,uri):return File(self,uri)
class Mount:
    def __init__(self,file):self.file=file;self.disconnect_request=None
    def get_root(self):return self.file
    def get_name(self):return 'sftp://user:never-publish-this@server/'
    def can_unmount(self):return True
    def unmount_with_operation(self,flags,operation,cancellable,callback,data):self.disconnect_request=(operation,cancellable,callback,data)
    def unmount_with_operation_finish(self,result):
        if isinstance(result,Exception):raise result
        return True
class Timers:
    def __init__(self):self.callbacks={};self.next=0
    def add(self,seconds,callback):self.next+=1;self.callbacks[self.next]=(seconds,callback);return self.next
    def remove(self,ident):self.callbacks.pop(ident,None)
class NetworkLocations(unittest.TestCase):
    def setUp(self):
        self.events=[];self.monitor=Monitor();self.provider=Provider();self.timers=Timers();self.toolkit=[]
        def operation():value=Operation();self.toolkit.append(value);return value
        self.backend=network.Backend(self.events.append,monitor=self.monitor,vfs=Vfs(),file_factory=self.provider.file,operation_factory=operation,schedule=self.timers.add,remove_timer=self.timers.remove)
    def connect(self,id=1):
        self.backend.dispatch({'id':id,'action':'connect','uri':'sftp://server/shared'})
        entry=next(reversed(self.backend.connections.values()));file,op,cancellable,callback,data=self.provider.mount_requests[-1];callback(file,True,data)
        file,cancellable,callback,data=self.provider.locate_requests[-1];mount=Mount(self.provider.file('sftp://server/'));callback(file,mount,data)
        return entry,mount
    def response(self,id):return next(event for event in reversed(self.events) if event.get('id')==id)
    def test_addresses_reject_credentials_queries_and_ambiguous_hosts_without_echo(self):
        self.assertEqual(network.address('SFTP://Example.COM/a b'),'sftp://example.com/a%20b')
        for value in ('sftp://user:hidden-secret@server/path','sftp://server/?token=hidden-secret','sftp://server/#hidden-secret','sftp://user%3Ahidden-secret%40server/path','file:///etc','sftp://server:99999/path','sftp://[invalid]/','sftp://server/%xx'):
            with self.subTest(value=value):
                with self.assertRaises(ValueError) as raised:network.address(value)
                self.assertNotIn('hidden-secret',str(raised.exception))
        self.assertEqual(network.address('sftp://user:hidden-secret@server/path',strip_identity=True),'sftp://server/path')
    def test_connect_authentication_keeps_questions_and_credentials_inside_toolkit(self):
        self.backend.dispatch({'id':1,'action':'connect','uri':'sftp://server/shared'})
        self.toolkit[0].emit('ask-password','question hidden-secret','user','domain',0)
        self.assertEqual(next(iter(self.backend.connections.values()))['state'],'authenticating')
        self.assertNotIn('hidden-secret',str(self.events))
        file,op,cancellable,callback,data=self.provider.mount_requests[-1];callback(file,True,data)
        file,cancellable,callback,data=self.provider.locate_requests[-1];mount=Mount(self.provider.file('sftp://server/'));callback(file,mount,data)
        self.assertTrue(self.response(1)['connected']);self.assertEqual(next(iter(self.backend.connections.values()))['state'],'connected')
    def test_cancel_and_timeout_ignore_late_success_and_dismiss_toolkit(self):
        self.backend.dispatch({'id':1,'action':'connect','uri':'sftp://server/'})
        file,op,cancellable,callback,data=self.provider.mount_requests[-1]
        self.backend.dispatch({'id':2,'action':'cancel','target':1})
        self.assertTrue(cancellable.is_cancelled());self.assertIn('aborted',op.emitted);self.assertTrue(self.response(1)['cancelled'])
        callback(file,True,data);self.assertFalse(self.provider.locate_requests)
        self.assertEqual(next(iter(self.backend.connections.values()))['state'],'disconnected')
        self.backend.dispatch({'id':3,'action':'connect','uri':'smb://server/share'})
        self.backend.timeout(3);self.assertIn('timed out',self.response(3)['error']);self.assertFalse(self.backend.operations)
    def test_browse_is_bounded_and_local_fuse_availability_is_explicit(self):
        entry,mount=self.connect();self.assertEqual(self.response(1)['path'],'')
        self.backend.dispatch({'id':2,'action':'browse','connectionId':entry['id']})
        file,cancellable,callback,data=self.provider.browse_requests[-1];enumerator=Enumerator([Info('file-'+str(i)) for i in range(120)]);callback(file,enumerator,data)
        result=self.response(2);self.assertEqual(len(result['rows']),100);self.assertTrue(result['limited']);self.assertEqual(result['path'],'');self.assertTrue(enumerator.closed)
        with self.assertRaises(ValueError):self.backend.dispatch({'id':3,'action':'browse','connectionId':entry['id'],'uri':'sftp://server/outside'})
    def test_new_browse_supersedes_old_and_never_displays_late_rows(self):
        entry,mount=self.connect();request={'action':'browse','connectionId':entry['id']}
        self.backend.dispatch({'id':2,**request});file,cancellable,callback,data=self.provider.browse_requests[-1]
        self.backend.dispatch({'id':3,**request});self.assertTrue(cancellable.is_cancelled())
        callback(file,Enumerator([Info('stale')]),data)
        self.assertTrue(self.response(2)['cancelled']);self.assertNotIn('rows',self.response(2))
        file,cancellable,callback,data=self.provider.browse_requests[-1];callback(file,Enumerator([Info('current')]),data)
        self.assertEqual(self.response(3)['rows'][0]['name'],'current')
    def test_mount_list_redacts_identity_and_unmount_reconnect_states_are_real(self):
        entry,mount=self.connect();self.monitor.mounts=[mount];self.backend.refresh_mounts()
        self.assertEqual(len(self.backend.connections),1);self.assertNotIn('never-publish-this',str(self.backend.snapshot()))
        self.backend.dispatch({'id':2,'action':'disconnect','connectionId':entry['id']});self.assertEqual(entry['state'],'disconnecting')
        op,cancellable,callback,data=mount.disconnect_request;callback(mount,True,data)
        self.assertEqual(entry['state'],'disconnected');self.assertTrue(self.response(2)['disconnected'])
        self.backend.dispatch({'id':3,'action':'reconnect','connectionId':entry['id']});self.assertEqual(entry['state'],'connecting')
    def test_unknown_credentials_fields_are_rejected_instead_of_stored(self):
        with self.assertRaises(ValueError):self.backend.dispatch({'id':1,'action':'connect','uri':'sftp://server/','password':'hidden-secret'})
        self.assertFalse(self.backend.connections);self.assertFalse(self.provider.mount_requests)
        self.assertNotIn('hidden-secret',str(self.events))
    def test_provider_absence_and_errors_never_claim_connection_or_echo_provider_text(self):
        self.backend.vfs.get_supported_uri_schemes=lambda:['file']
        self.assertFalse(self.backend.snapshot()['available'])
        with self.assertRaises(ValueError):self.backend.dispatch({'id':1,'action':'connect','uri':'sftp://server/'})
        error=GLib.Error.new_literal(Gio.io_error_quark(),'sftp://user:hidden-secret@server failed',Gio.IOErrorEnum.PERMISSION_DENIED)
        self.assertNotIn('hidden-secret',network.failure(error));self.assertIn('denied',network.failure(error))
    def test_disconnected_mount_cancels_pending_directory_read(self):
        entry,mount=self.connect();self.monitor.mounts=[mount];self.backend.refresh_mounts();self.backend.dispatch({'id':2,'action':'browse','connectionId':entry['id']})
        self.monitor.mounts=[];self.backend.refresh_mounts();self.assertEqual(entry['state'],'disconnected');self.assertTrue(self.response(2)['cancelled']);self.assertFalse(self.backend.operations)
if __name__=='__main__':unittest.main()

"""Disconnected, replaced and regressed native authority is never an unlock signal."""
import importlib.util,json,os,socket,struct,tempfile,threading,time,unittest
from pathlib import Path
spec=importlib.util.spec_from_file_location('native_lock_authority',Path(__file__).resolve().parents[2]/'res/lumen-desktop/notifications/session_lock.py');module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)
class NativePeer:
 def __init__(self,path):
  self.path=path;self.payload=None;self.locked=False;self.revision=0;self.subscribers=[];self.stopped=False
  self.listener=socket.socket(socket.AF_UNIX,socket.SOCK_STREAM);self.listener.bind(path);self.listener.listen();self.listener.settimeout(.1)
  self.thread=threading.Thread(target=self.run,daemon=True);self.thread.start()
 def run(self):
  while not self.stopped:
   try:connection,_=self.listener.accept()
   except socket.timeout:continue
   except OSError:return
   try:
    header=connection.recv(14);magic,size,kind=module.HEADER.unpack(header)
    while size:chunk=connection.recv(size);size-=len(chunk)
    value={'success':True} if kind==2 else {'lumen_session_lock':{'version':1,'available':True,'locked':self.locked,'revision':self.revision}}
    if kind==7 and self.payload is not None:value=self.payload
    body=json.dumps(value).encode();connection.sendall(module.HEADER.pack(module.MAGIC,len(body),kind)+body)
    if kind==2:self.subscribers.append(connection)
    else:connection.close()
   except OSError:connection.close()
 def stop(self):
  self.stopped=True;self.listener.close()
  for connection in self.subscribers:connection.close()
  self.thread.join(timeout=1);Path(self.path).unlink(missing_ok=True)
class AuthorityTests(unittest.TestCase):
 def test_policy_needs_known_native_unlock_or_explicit_host_permission(self):
  allow=module.previews_allowed
  for preview in ('always','when_unlocked','never'):
   self.assertFalse(allow(preview,{'known':False,'locked':False},True))
  self.assertTrue(allow('when_unlocked',{'known':True,'locked':False}))
  self.assertFalse(allow('when_unlocked',{'known':True,'locked':True},True))
  self.assertFalse(allow('always',{'known':True,'locked':True}))
  self.assertTrue(allow('always',{'known':True,'locked':True},True))
  self.assertFalse(allow('never',{'known':True,'locked':False},True))
 def test_unknown_and_malformed_native_facts_fail_closed(self):
  with tempfile.TemporaryDirectory(prefix='lock-malformed-') as private:
   path=str(Path(private)/'ipc');peer=NativePeer(path)
   try:
    good={'version':1,'available':True,'locked':False,'revision':0}
    invalid=[[],{}, {'lumen_session_lock':[]}, *({'lumen_session_lock':dict(good,**{key:value})} for key,value in [('version',True),('version',2),('available',False),('locked',0),('revision',True),('revision',-1),('revision',1.5)])]
    for payload in invalid:
     with self.subTest(payload=payload):
      peer.payload=payload;authority=module.SessionLockAuthority(path=path)
      try:self.assertEqual(authority.snapshot(),{'known':False,'locked':True})
      finally:authority.close()
   finally:peer.stop()
 def test_peer_disconnect_revision_regression_and_replacement(self):
  with tempfile.TemporaryDirectory(prefix='lock-authority-') as private:
   path=str(Path(private)/'ipc');authority=module.SessionLockAuthority(path=path);self.assertFalse(authority.snapshot()['known'])
   peer=NativePeer(path)
   try:
    authority.retry=0;self.assertEqual(authority.snapshot(),{'known':True,'locked':False})
    peer.locked=True;peer.revision=1;self.assertEqual(authority.snapshot(),{'known':True,'locked':True})
    old_connection=authority.monitor
    class EventFlags:
     IO_HUP=16;IO_ERR=8
     @staticmethod
     def source_remove(source):pass
    authority.glib=EventFlags
    body=json.dumps({'payload':'unlocked','locked':False}).encode()
    peer.subscribers[0].sendall(module.HEADER.pack(module.MAGIC,len(body),0x80000007)+body)
    self.assertTrue(authority._events(None,0,authority.monitor));self.assertEqual(authority.facts,{'known':True,'locked':True})
    authority.glib=None
    peer.revision=0;self.assertEqual(authority.snapshot(),{'known':False,'locked':True})
    peer.stop();peer=NativePeer(path);peer.locked=True
    authority.retry=0;self.assertEqual(authority.snapshot(),{'known':True,'locked':True})
    self.assertFalse(authority._events(None,0,old_connection));self.assertEqual(authority.facts,{'known':True,'locked':True})
    peer.stop();self.assertEqual(authority.snapshot(),{'known':False,'locked':True})
   finally:authority.close();peer.stop()
if __name__=='__main__':unittest.main()

"""Real service/client lifetime and cooperative shutdown with explicit copy gates."""
import json,os,socket,subprocess,sys,tempfile,time,unittest
from pathlib import Path
from unittest.mock import patch
sys.path.insert(0,str(Path(__file__).resolve().parents[2]/'res/lumen-desktop/shell'))
import file_jobs

class FileServiceTests(unittest.TestCase):
 def wait(self,predicate,server):
  deadline=time.monotonic()+8
  while time.monotonic()<deadline:
   self.assertIsNone(server.poll(),'File service exited before handshake')
   try:
    value=predicate()
    if value:return value
   except (FileNotFoundError,ConnectionRefusedError):pass
   time.sleep(.01)
  self.fail('File service handshake did not complete')
 def stop(self,server):
  if server.poll() is None:server.terminate()
  try:server.wait(timeout=8)
  except subprocess.TimeoutExpired:
   server.kill();server.wait();self.fail('File service did not finish cooperative cleanup')
 def launch(self,root,hold_state_lock):
  # Entered is written only after the real job has created its staging directory.
  # The worker then waits for an explicit release or its real cancellation token.
  code="""import file_jobs,time
from pathlib import Path
original=file_jobs.Jobs.copy
entered=Path(%r);release=Path(%r);hold=Path(%r);locked=Path(%r);hold_on_cancel=%r
def gated(self,ident,*args):
 if not entered.exists():
  entered.write_text('ready')
  if hold_on_cancel:
   while not hold.exists() and not self.tokens[ident].wait(.01):pass
   with self.lock:
    locked.write_text('ready')
    self.tokens[ident].wait(8)
  else:
   while not release.exists() and not self.tokens[ident].wait(.01):pass
 return original(self,ident,*args)
file_jobs.Jobs.copy=gated
original_request=file_jobs.request
def gated_response(data):
 if data.get('testDisconnect'):
  (entered.parent/'response-entered').write_text('ready')
  while not (entered.parent/'response-release').exists():time.sleep(.01)
 return original_request(data)
file_jobs.request=gated_response
file_jobs.serve()
"""%(str(root/'entered'),str(root/'release'),str(root/'hold'),str(root/'locked'),hold_state_lock)
  server=subprocess.Popen([sys.executable,'-c',code],env={**os.environ,'PYTHONPATH':str(Path(file_jobs.__file__).parent)},stdout=subprocess.DEVNULL,stderr=subprocess.PIPE,text=True)
  self.addCleanup(self.cleanup,server)
  self.wait(lambda:file_jobs.remote({'action':'job-status'}),server)
  return server
 def cleanup(self,server):
  try:self.stop(server)
  finally:
   if server.stderr:server.stderr.close()
 def test_client_lifetime_and_shutdown(self):
  with tempfile.TemporaryDirectory() as t,patch.dict(os.environ,{'XDG_RUNTIME_DIR':t,'XDG_STATE_HOME':str(Path(t)/'state'),'XDG_DATA_HOME':str(Path(t)/'data'),'LUMEN_FILE_SERVICE':'1'}):
   root=Path(t);source=root/'source';destination=root/'destination';source.mkdir();destination.mkdir()
   for i in range(4):(source/str(i)).write_bytes(b'fixture'+bytes([i]))
   server=self.launch(root,False)
   try:
    script='import file_jobs;print(file_jobs.remote('+repr(dict(action='job-start',operation='copy',paths=[str(source)],destination=str(destination)))+')["job"])'
    client=subprocess.run([sys.executable,'-c',script],env={**os.environ,'PYTHONPATH':str(Path(file_jobs.__file__).parent)},check=True,text=True,capture_output=True,timeout=8)
    ident=client.stdout.strip();self.assertTrue(ident)
    self.wait(lambda:(root/'entered').exists(),server)
    self.assertEqual(file_jobs.remote({'action':'job-status'})['jobs'][-1]['state'],'running')
    self.assertFalse((destination/'source').exists())
    (root/'release').write_text('continue')
    def terminal():
     job=next(j for j in file_jobs.remote({'action':'job-status'})['jobs'] if j['id']==ident)
     return job if job['state'] not in ('running','cancelling') else None
    job=self.wait(terminal,server);self.assertEqual(job['state'],'complete')
    self.assertEqual({p.name:p.read_bytes() for p in (destination/'source').iterdir()},{p.name:p.read_bytes() for p in source.iterdir()})
    self.stop(server);self.assertEqual(server.returncode,0)
    self.assertFalse(list(destination.glob('.lumen-copy-*')));self.assertFalse((root/'lumen-file-operations.sock').exists())
   finally:self.stop(server)
 def test_disconnected_client_does_not_stop_service(self):
  with tempfile.TemporaryDirectory() as t,patch.dict(os.environ,{'XDG_RUNTIME_DIR':t,'XDG_STATE_HOME':str(Path(t)/'state'),'XDG_DATA_HOME':str(Path(t)/'data'),'LUMEN_FILE_SERVICE':'1'}):
   root=Path(t);server=self.launch(root,False)
   try:
    with socket.socket(socket.AF_UNIX,socket.SOCK_STREAM) as client:
     client.connect(str(root/'lumen-file-operations.sock'))
     client.sendall(json.dumps({'action':'job-status','testDisconnect':True}).encode()+b'\n')
     self.wait(lambda:(root/'response-entered').exists(),server)
     client.shutdown(socket.SHUT_RDWR)
    (root/'response-release').write_text('client closed')
    self.assertEqual(self.wait(lambda:file_jobs.remote({'action':'job-status'}),server)['jobs'],[])
    self.stop(server);self.assertEqual(server.returncode,0)
   finally:self.stop(server)
 def test_service_stop_cleans_unfinished_copy(self):
  with tempfile.TemporaryDirectory() as t,patch.dict(os.environ,{'XDG_RUNTIME_DIR':t,'XDG_STATE_HOME':str(Path(t)/'state'),'XDG_DATA_HOME':str(Path(t)/'data'),'LUMEN_FILE_SERVICE':'1'}):
   root=Path(t);source=root/'source';destination=root/'destination';source.mkdir();destination.mkdir();(source/'keep').write_text('source')
   server=self.launch(root,True)
   try:
    file_jobs.remote(dict(action='job-start',operation='copy',paths=[str(source)],destination=str(destination)))
    self.wait(lambda:(root/'entered').exists(),server)
    (root/'hold').write_text('hold state lock')
    self.wait(lambda:(root/'locked').exists(),server)
    self.assertTrue(list(destination.glob('.lumen-copy-*')))
    self.stop(server);self.assertEqual(server.returncode,0)
    self.assertEqual(list(destination.iterdir()),[]);self.assertEqual((source/'keep').read_text(),'source')
    self.assertFalse((root/'lumen-file-operations.sock').exists())
    journal=json.loads((root/'state/lumen-desktop/operations/file-jobs.json').read_text());self.assertEqual(journal[-1]['state'],'cancelled')
   finally:self.stop(server)
if __name__=='__main__':unittest.main()

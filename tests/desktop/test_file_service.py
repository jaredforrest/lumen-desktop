"""The file-job service survives request/client exit and stops without losing source data."""
import json,os,subprocess,sys,tempfile,time,unittest
from pathlib import Path
from unittest.mock import patch
sys.path.insert(0,str(Path(__file__).resolve().parents[2]/'res/lumen-desktop/shell'))
import file_jobs
class FileServiceTests(unittest.TestCase):
 def test_client_lifetime_and_shutdown(self):
  with tempfile.TemporaryDirectory() as t,patch.dict(os.environ,{'XDG_RUNTIME_DIR':t,'LUMEN_FILE_SERVICE':'1'}):
   root=Path(t);source=root/'source';destination=root/'destination';source.mkdir();destination.mkdir()
   for i in range(60):(source/str(i)).write_bytes(b'x'*1048576)
   server=subprocess.Popen([sys.executable,file_jobs.__file__,'serve'],stdout=subprocess.DEVNULL)
   try:
    deadline=time.monotonic()+5
    while not (root/'lumen-file-operations.sock').exists() and time.monotonic()<deadline:time.sleep(.02)
    # The initiating process exits immediately; it does not own the operation.
    # API action uses job-start, matching the Files transport.
    script='import file_jobs;file_jobs.remote('+repr(dict(action='job-start',operation='copy',paths=[str(source)],destination=str(destination)))+')'
    subprocess.run([sys.executable,'-c',script],env={**os.environ,'PYTHONPATH':str(Path(file_jobs.__file__).parent)},check=True)
    deadline=time.monotonic()+10
    while time.monotonic()<deadline:
     state=file_jobs.remote({'action':'job-status'});job=state['jobs'][-1]
     if job['state']!='running':break
     time.sleep(.02)
    self.assertEqual(job['state'],'complete');self.assertEqual(len(list((destination/'source').iterdir())),60)
    self.assertEqual(len(list(source.iterdir())),60)
    server.terminate();server.wait(timeout=10)
    self.assertFalse(list(destination.glob('.lumen-copy-*')))
   finally:
    if server.poll() is None:server.terminate();server.wait(timeout=10)
 def test_service_stop_cleans_unfinished_copy(self):
  with tempfile.TemporaryDirectory() as t,patch.dict(os.environ,{'XDG_RUNTIME_DIR':t,'LUMEN_FILE_SERVICE':'1'}):
   root=Path(t);source=root/'source';destination=root/'destination';source.mkdir();destination.mkdir();(source/'keep').write_text('source')
   # Delay the real copy at a deterministic point to deliver SIGTERM in flight.
   code="import file_jobs,time; original=file_jobs.Jobs.copy\ndef delayed(self,*args):\n time.sleep(.4);return original(self,*args)\nfile_jobs.Jobs.copy=delayed\nfile_jobs.serve()"
   server=subprocess.Popen([sys.executable,'-c',code],env={**os.environ,'PYTHONPATH':str(Path(file_jobs.__file__).parent)})
   try:
    deadline=time.monotonic()+5
    while not (root/'lumen-file-operations.sock').exists() and time.monotonic()<deadline:time.sleep(.02)
    file_jobs.remote(dict(action='job-start',operation='copy',paths=[str(source)],destination=str(destination)))
    deadline=time.monotonic()+3
    while not list(destination.glob('.lumen-copy-*')) and time.monotonic()<deadline:time.sleep(.01)
    self.assertTrue(list(destination.glob('.lumen-copy-*')))
    server.terminate();server.wait(timeout=5)
    self.assertEqual(list(destination.iterdir()),[]);self.assertEqual((source/'keep').read_text(),'source')
   finally:
    if server.poll() is None:server.terminate();server.wait(timeout=5)
if __name__=='__main__':unittest.main()

"""Fixed content supervisor cancels its own private helper session, not bystander jobs."""
import json,os,shutil,signal,subprocess,sys,tempfile,time,unittest
from pathlib import Path
ROOT=Path(__file__).resolve().parents[2]
class Cleanup(unittest.TestCase):
 def test_actual_parent_cancellation_stops_owned_helper_and_descendant(self):
  with tempfile.TemporaryDirectory() as temporary:
   home=Path(temporary);shutil.copy2(ROOT/'res/lumen-desktop/shell/captured_resource_actions.py',home/'captured_resource_actions.py')
   (home/'context_tools.py').write_text('''import json,os,subprocess,sys,time
from pathlib import Path
request=json.loads(sys.stdin.readline());child=subprocess.Popen([sys.executable,'-c','import time;time.sleep(120)'])
Path(request['path']).write_text(json.dumps([os.getpid(),child.pid]))
time.sleep(120)
''')
   worker=home/'worker.py';worker.write_text('''import signal,sys
from captured_resource_actions import run_content
def cancel(*_):raise SystemExit(0)
signal.signal(signal.SIGTERM,cancel)
run_content(dict(action='ocr',path=sys.argv[1]))
''')
   marker=home/'pids';bystander=subprocess.Popen([sys.executable,'-c','import time;time.sleep(120)'],start_new_session=True);child=subprocess.Popen([sys.executable,str(worker),str(marker)],start_new_session=True)
   try:
    deadline=time.monotonic()+5
    while not marker.exists() and time.monotonic()<deadline:time.sleep(.02)
    helper,descendant=json.loads(marker.read_text());child.terminate();self.assertEqual(child.wait(timeout=5),0)
    self.assertFalse(Path('/proc/'+str(helper)).exists(),'owned helper was not reaped')
    deadline=time.monotonic()+3
    def running(pid):
     try:return Path('/proc/'+str(pid)+'/stat').read_text().rsplit(') ',1)[1].split()[0]!='Z'
     except FileNotFoundError:return False
    while running(descendant) and time.monotonic()<deadline:time.sleep(.02)
    self.assertFalse(running(descendant));self.assertIsNone(bystander.poll())
   finally:
    for proc in (child,bystander):
     if proc.poll() is None:proc.kill()
     proc.wait(timeout=5)
 def test_subject_export_retains_exact_output_bytes(self):
  import sys
  from unittest.mock import patch
  sys.path.insert(0,str(ROOT/'res/lumen-desktop/shell'))
  from context_tools import subject_stream,save_subject
  from captured_resource_actions import facts
  with tempfile.TemporaryDirectory() as temporary,patch.dict(os.environ,{'XDG_CACHE_HOME':temporary}):
   root=Path(temporary)/'lumen-desktop/subjects';root.mkdir(parents=True);source=root/('a'*32+'.png');source.write_bytes(b'captured subject');expected=facts(source)
   with subject_stream(source,expected) as stream:
    source.write_bytes(b'later mutation');self.assertEqual(stream.read(),b'captured subject')
   destination=Path(temporary)/'export.png'
   with self.assertRaises(ValueError):save_subject(source,destination,False,expected)
   self.assertFalse(destination.exists())
if __name__=='__main__':unittest.main()

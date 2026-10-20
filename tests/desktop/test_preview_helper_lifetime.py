"""A blocked preview helper expires even when its Python caller is killed."""
import json,os,re,shutil,signal,subprocess as sp,sys,tempfile,time,unittest
from pathlib import Path
class HelperLifetimeTests(unittest.TestCase):
 def test_owned_wall_deadline_survives_caller_death(self):
  launcher=os.environ.get('LUMEN_TEST_SHELL_LAUNCHER') or shutil.which('lumen-desktop-shell')
  self.assertIsNotNone(launcher,'Installed shell runtime is required')
  prefix=re.split(r'\n[ \t]*exec quickshell ',Path(launcher).read_text(),maxsplit=1)[0]
  with tempfile.TemporaryDirectory(prefix='lumen-preview-lifetime-') as tmp:
   home=Path(tmp);env=dict(os.environ,HOME=tmp,XDG_RUNTIME_DIR=tmp)
   for key in ('SWAYSOCK','WAYLAND_DISPLAY','DISPLAY','DBUS_SESSION_BUS_ADDRESS'):env.pop(key,None)
   env.update(dict(v.decode().split('=',1) for v in sp.check_output(['bash','-c',prefix+'\nexec env -0'],env=env).split(b'\0') if b'=' in v))
   timeout=shutil.which('timeout',path=env['PATH']);self.assertTrue(timeout and timeout.startswith('/nix/store/'),timeout)
   source=Path(__file__).resolve().parents[2]/'res/lumen-desktop/shell'
   marker=home/'helper.json';child=home/'blocked.py';child.write_text('import json,os,time\nfrom pathlib import Path\nPath('+repr(str(marker))+').write_text(json.dumps(dict(pid=os.getpid(),parent=os.getppid(),group=os.getpgrp())))\ntime.sleep(60)\n')
   caller=home/'caller.py';caller.write_text('import sys\nsys.path.insert(0,'+repr(str(source))+')\nfrom file_preview import limited_command\nlimited_command(['+repr(sys.executable)+','+repr(str(child))+'])\n')
   processes=[]
   def alive(pid):
    try:return Path('/proc/'+str(pid)+'/stat').read_text().split(') ',1)[1][0]!='Z'
    except FileNotFoundError:return False
   helper_pid=None
   try:
    sentinel=sp.Popen([sys.executable,'-c','import time;time.sleep(60)'],env=env);processes.append(sentinel)
    with (home/'caller.log').open('w') as log:parent=sp.Popen([sys.executable,str(caller)],env=env,stdout=log,stderr=sp.STDOUT)
    processes.append(parent);until=time.monotonic()+5
    while not marker.exists() and time.monotonic()<until:time.sleep(.02)
    facts=json.loads(marker.read_text());helper_pid=facts['pid'];self.assertEqual(facts['parent'],facts['group']);self.assertNotEqual(facts['group'],os.getpgrp());self.assertNotEqual(facts['group'],parent.pid)
    os.kill(parent.pid,signal.SIGKILL);parent.wait(timeout=3);until=time.monotonic()+9
    while alive(helper_pid) and time.monotonic()<until:time.sleep(.04)
    self.assertFalse(alive(helper_pid),'Independent timeout did not retire blocked orphan helper');self.assertIsNone(sentinel.poll(),'Preview timeout affected an unrelated child')
   finally:
    if helper_pid and alive(helper_pid):os.kill(helper_pid,signal.SIGKILL)
    for proc in processes:
     if proc.poll() is None:proc.kill()
     proc.wait(timeout=3)
if __name__=='__main__':unittest.main()

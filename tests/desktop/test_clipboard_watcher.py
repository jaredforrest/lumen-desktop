"""Exercise the collector supervisor with fake offer transport, never a display."""
import importlib.util
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import tempfile
import time
import unittest

SOURCE=Path(__file__).resolve().parents[2]/'res/lumen-desktop/runtime/clipboard_history.py'
spec=importlib.util.spec_from_file_location('clipboard_history',SOURCE);m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m)


class WatcherLifecycle(unittest.TestCase):
    def test_native_transport_stops_after_revocation(self):
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp);runtime=root/'runtime';runtime.mkdir(mode=0o700)
            store=m.History(runtime/'lumen-desktop/clipboard',root/'state/lumen-desktop/clipboard',retention=60)
            store.set_enabled(True)
            binary=root/'bin';binary.mkdir()
            fake=binary/'native-observer'
            fake.write_text('#!'+sys.executable+'\nimport sys,time\npayload=time.clock_gettime_ns(time.CLOCK_BOOTTIME).to_bytes(8,"big")+b"synthetic watcher fixture"\nsys.stdout.buffer.write(b"\\x01"+bytes(4)+b"\\x02"+len(payload).to_bytes(4,"big")+payload);sys.stdout.buffer.flush()\ntime.sleep(30)\n')
            fake.chmod(0o700)
            env=dict(os.environ,PATH=str(binary)+':'+os.environ['PATH'],XDG_RUNTIME_DIR=str(runtime),XDG_STATE_HOME=str(root/'state'),LUMEN_CLIPBOARD_RETENTION_SECONDS='60',LUMEN_CLIPBOARD_OBSERVER=str(fake))
            with subprocess.Popen([sys.executable,str(SOURCE),'watch'],env=env,stdout=subprocess.PIPE,stderr=subprocess.PIPE,start_new_session=True) as process:
                try:
                    end=time.monotonic()+5
                    while time.monotonic()<end and not store.list()['entries']:
                        if process.poll() is not None:self.fail('Synthetic watcher exited early')
                        time.sleep(.05)
                    self.assertEqual(store.list()['entries'][0]['text'],'synthetic watcher fixture')
                    store.set_enabled(False);process.wait(timeout=4)
                    self.assertEqual(store.list()['entries'],[])
                finally:
                    try:os.killpg(process.pid,signal.SIGTERM)
                    except ProcessLookupError:pass
                    process.wait(timeout=4)
    def test_clear_start_failure_revokes_consent(self):
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp);runtime=root/'runtime';runtime.mkdir(mode=0o700);binary=root/'bin';binary.mkdir()
            store=m.History(runtime/'lumen-desktop/clipboard',root/'state/lumen-desktop/clipboard');store.set_enabled(True);store.ingest('synthetic old')
            fake=binary/'systemctl';fake.write_text('#!'+sys.executable+'\nimport sys\nfrom pathlib import Path\nwith Path('+repr(str(root/'calls'))+').open("a") as f:f.write(sys.argv[2]+"\\n")\nraise SystemExit(1 if sys.argv[2]=="start" else 0)\n');fake.chmod(0o700)
            env=dict(os.environ,PATH=str(binary)+':'+os.environ['PATH'],XDG_RUNTIME_DIR=str(runtime),XDG_STATE_HOME=str(root/'state'))
            result=subprocess.run([sys.executable,str(SOURCE),'clear'],env=env,capture_output=True,text=True)
            self.assertNotEqual(result.returncode,0);self.assertFalse(store.enabled());self.assertEqual(store.list()['entries'],[])
            self.assertEqual((root/'calls').read_text().splitlines(),['stop','start'])
    def test_clear_does_not_restart_after_concurrent_revocation(self):
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp);runtime=root/'runtime';runtime.mkdir(mode=0o700);binary=root/'bin';binary.mkdir()
            store=m.History(runtime/'lumen-desktop/clipboard',root/'state/lumen-desktop/clipboard');store.set_enabled(True);store.ingest('synthetic old')
            fake=binary/'systemctl';fake.write_text('#!'+sys.executable+'\nimport sys\nfrom pathlib import Path\nassert sys.argv[2]=="stop"\nPath('+repr(str(store.consent))+').unlink()\n');fake.chmod(0o700)
            env=dict(os.environ,PATH=str(binary)+':'+os.environ['PATH'],XDG_RUNTIME_DIR=str(runtime),XDG_STATE_HOME=str(root/'state'))
            result=subprocess.run([sys.executable,str(SOURCE),'clear'],env=env,capture_output=True,text=True)
            self.assertEqual(result.returncode,0,result.stderr);self.assertFalse(store.enabled());self.assertEqual(store.list()['entries'],[])
    def test_stop_failure_revokes_and_supervisor_reaps_idle_observer(self):
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp);runtime=root/'runtime';runtime.mkdir(mode=0o700);binary=root/'bin';binary.mkdir()
            store=m.History(runtime/'lumen-desktop/clipboard',root/'state/lumen-desktop/clipboard');store.set_enabled(True)
            fake=binary/'observer';fake.write_text('#!'+sys.executable+'\nimport os,sys,time\nfrom pathlib import Path\nPath('+repr(str(root/'observer.pid'))+').write_text(str(os.getpid()))\nsys.stdout.buffer.write(b"\\x01"+bytes(4));sys.stdout.buffer.flush()\ntime.sleep(30)\n');fake.chmod(0o700)
            ctl=binary/'systemctl';ctl.write_text('#!'+sys.executable+'\nraise SystemExit(1)\n');ctl.chmod(0o700)
            env=dict(os.environ,PATH=str(binary)+':'+os.environ['PATH'],XDG_RUNTIME_DIR=str(runtime),XDG_STATE_HOME=str(root/'state'),LUMEN_CLIPBOARD_OBSERVER=str(fake))
            with subprocess.Popen([sys.executable,str(SOURCE),'watch'],env=env,stdout=subprocess.PIPE,stderr=subprocess.PIPE,start_new_session=True) as process:
                try:
                    end=time.monotonic()+4
                    while not (root/'observer.pid').exists() and time.monotonic()<end:time.sleep(.02)
                    self.assertTrue((root/'observer.pid').exists());observer=int((root/'observer.pid').read_text())
                    result=subprocess.run([sys.executable,str(SOURCE),'clear'],env=env,capture_output=True,text=True)
                    self.assertNotEqual(result.returncode,0);self.assertFalse(store.enabled());process.wait(timeout=3)
                    with self.assertRaises(ProcessLookupError):os.kill(observer,0)
                    self.assertEqual(store.list()['entries'],[])
                finally:
                    if process.poll() is None:os.killpg(process.pid,signal.SIGTERM);process.wait(timeout=4)
    def test_enable_failure_rolls_back_consent_without_collecting(self):
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp);runtime=root/'runtime';runtime.mkdir(mode=0o700);binary=root/'bin';binary.mkdir()
            fake=binary/'systemctl';fake.write_text('#!'+sys.executable+'\nraise SystemExit(1)\n');fake.chmod(0o700)
            env=dict(os.environ,PATH=str(binary)+':'+os.environ['PATH'],XDG_RUNTIME_DIR=str(runtime),XDG_STATE_HOME=str(root/'state'))
            result=subprocess.run([sys.executable,str(SOURCE),'enable'],env=env,capture_output=True,text=True)
            self.assertNotEqual(result.returncode,0)
            self.assertFalse(json.loads(result.stdout)['enabled'])
            self.assertFalse((root/'state/lumen-desktop/clipboard/consent').exists())


if __name__=='__main__':unittest.main()

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
    def test_watch_uses_absolute_fallback_and_stops_after_revocation(self):
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp);runtime=root/'runtime';runtime.mkdir(mode=0o700)
            store=m.History(runtime/'lumen-desktop/clipboard',root/'state/lumen-desktop/clipboard',retention=60)
            store.set_enabled(True)
            binary=root/'bin';binary.mkdir()
            fake=binary/'wl-paste'
            fake.write_text('#!'+sys.executable+'\nimport subprocess,sys,time\nif "--list-types" in sys.argv:print("text/plain;charset=utf-8")\nelse:\n i=sys.argv.index("--watch")\n subprocess.run(sys.argv[i+1:],input=b"synthetic watcher fixture",check=True)\n time.sleep(30)\n')
            fake.chmod(0o700)
            env=dict(os.environ,PATH=str(binary)+':'+os.environ['PATH'],XDG_RUNTIME_DIR=str(runtime),XDG_STATE_HOME=str(root/'state'),LUMEN_CLIPBOARD_RETENTION_SECONDS='60')
            env.pop('LUMEN_CLIPBOARD_COMMAND',None)
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

"""Actual namespace boundary, independent of optional browser/native rendering."""
import json,os,sys,tempfile,unittest
from pathlib import Path
sys.path.insert(0,str(Path(__file__).resolve().parent/'application_fixture'))
import isolation
ROOT=Path(__file__).resolve().parents[2]
class Isolation(unittest.TestCase):
 def test_existing_or_insecure_output_is_rejected_before_namespace_setup(self):
  with tempfile.TemporaryDirectory() as temporary:
   root=Path(temporary);(root/'existing').write_text('unchanged')
   with self.assertRaises(ValueError):isolation.run(ROOT,root,[sys.executable,'-c','raise SystemExit(99)'])
   self.assertEqual((root/'existing').read_text(),'unchanged')
   (root/'existing').unlink();root.chmod(0o777)
   with self.assertRaises(ValueError):isolation.run(ROOT,root,[sys.executable,'-c','raise SystemExit(99)'])
 def test_actual_nonroot_private_filesystem_process_and_loopback_only(self):
  with tempfile.TemporaryDirectory(prefix='lumen-browser-probe-') as temporary:
   root=Path(temporary);out=root/'output';out.mkdir();sentinel=root/'must-not-be-visible';sentinel.write_text('synthetic outside boundary')
   code='''import json,os,socket,threading,subprocess,sys
from pathlib import Path
assert os.getuid()!=0
assert socket.gethostname()=="fixture"
assert "FIXTURE_SECRET" not in os.environ and "LD_PRELOAD" not in os.environ
assert not Path(%r).exists()
assert [path.name for path in Path('/home').iterdir()]==['fixture']
assert not Path('/persist').exists()
assert not Path('/dev/dri').exists() and not Path('/dev/snd').exists()
try:
 with Path('/source/README.md').open('a'):pass
 raise AssertionError('Source was writable')
except OSError:pass
status=Path('/proc/self/status').read_text()
assert 'CapEff:\\t0000000000000000' in status and 'CapBnd:\\t0000000000000000' in status
nested=subprocess.run([sys.executable,'-c',"import ctypes; library=ctypes.CDLL(None,use_errno=True); result=library.unshare(0x10000000); assert result==0,(result,ctypes.get_errno())"],text=True,capture_output=True,timeout=2)
assert nested.returncode==0,nested.stderr
assert not Path('/.old-root').exists()
assert all(line.split()[1]!='00000000' for line in Path('/proc/net/route').read_text().splitlines()[1:])
with socket.socket() as listener:
 listener.bind(('127.0.0.1',0));listener.listen(1)
 def serve():
  connection,_=listener.accept()
  with connection:connection.sendall(b'private loopback')
 worker=threading.Thread(target=serve);worker.start()
 with socket.create_connection(listener.getsockname(),timeout=2) as client:assert client.recv(128)==b'private loopback'
 worker.join(2);assert not worker.is_alive()
Path('/artifacts/result.json').write_text(json.dumps(dict(passed=True,uid=os.getuid(),pid=os.getpid(),network=Path('/proc/net/dev').read_text())))
'''%str(sentinel)
   result=isolation.run(ROOT,out,[sys.executable,'-c',code],dict(os.environ,FIXTURE_SECRET='synthetic omitted value'));self.assertEqual(result,0);proof=json.loads((out/'result.json').read_text());self.assertTrue(proof['passed']);self.assertEqual(sentinel.read_text(),'synthetic outside boundary')
if __name__=='__main__':unittest.main()

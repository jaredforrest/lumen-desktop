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
if __name__=='__main__':unittest.main()

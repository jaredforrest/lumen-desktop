"""Actual namespace boundary, independent of optional browser/native rendering."""
import json,os,sys,tempfile,unittest
from pathlib import Path
from unittest.mock import patch
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
 def test_private_terminal_policy_is_explicit_and_typed(self):
  for value in (None,0,1,'yes',[]):
   with self.subTest(value=value),self.assertRaises(ValueError),patch.object(isolation.subprocess,'call') as child:
    isolation.run(ROOT,ROOT,[sys.executable],private_pty=value)
   child.assert_not_called()
  with tempfile.TemporaryDirectory() as temporary:
   root=Path(temporary);root.chmod(0o700)
   def inspect(command):
    descriptor=json.loads(Path(command[-1]).read_text())
    self.assertIs(descriptor['private_pty'],True)
    self.assertIs(descriptor['software_graphics'],False)
    return 0
   with patch.object(isolation.subprocess,'call',side_effect=inspect):
    self.assertEqual(isolation.run(ROOT,root,[sys.executable],private_pty=True),0)

if __name__=='__main__':unittest.main()

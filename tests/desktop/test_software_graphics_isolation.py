"""Optional software allocator policy, independent of default application isolation."""
import importlib.util,os,stat,unittest
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch
spec=importlib.util.spec_from_file_location('isolation',Path(__file__).parent/'application_fixture/isolation.py');module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)
class SoftwareGraphics(unittest.TestCase):
 def info(self,mode=stat.S_IFCHR,rdev=None,inode=5):return SimpleNamespace(st_mode=mode,st_dev=7,st_ino=inode,st_rdev=os.makedev(10,267) if rdev is None else rdev)
 def text(self,path):return '10:267\n' if str(path).endswith('/dev') else 'MAJOR=10\nMINOR=267\nDEVNAME=udmabuf\n'
 def test_typed_default_off(self):
  self.assertFalse(module.graphics_policy(False));self.assertTrue(module.graphics_policy(True))
  for value in (0,1,None,'true',[]):
   with self.assertRaises(ValueError):module.graphics_policy(value)
 def test_exact_accessible_character_device(self):
  with patch.object(Path,'lstat',return_value=self.info()),patch.object(Path,'read_text',lambda path:self.text(path)),patch.object(os,'access',return_value=True):self.assertEqual(module.software_device(),(7,5,os.makedev(10,267)))
 def test_noncharacter_or_symlink_refused(self):
  for mode in (stat.S_IFLNK,stat.S_IFREG):
   with patch.object(Path,'lstat',return_value=self.info(mode)),self.assertRaises(ValueError):module.software_device()
 def test_sysfs_or_access_mismatch_refused(self):
  with patch.object(Path,'lstat',return_value=self.info()),patch.object(Path,'read_text',return_value='wrong'),self.assertRaises(ValueError):module.software_device()
  with patch.object(Path,'lstat',return_value=self.info()),patch.object(Path,'read_text',lambda path:self.text(path)),patch.object(os,'access',return_value=False),self.assertRaises(PermissionError):module.software_device()
 def test_bind_rechecks_source_and_target(self):
  identity=(7,5,os.makedev(10,267))
  with patch.object(module,'software_device',return_value=identity),patch.object(module,'bind') as bind,patch.object(Path,'lstat',return_value=self.info()):module.bind_software_device('/private/dev/udmabuf');bind.assert_called_once_with('/dev/udmabuf','/private/dev/udmabuf')
  for target,source in ((self.info(inode=6),identity),(self.info(),(7,6,identity[2]))):
   with patch.object(module,'software_device',side_effect=[identity,source]),patch.object(module,'bind'),patch.object(Path,'lstat',return_value=target),self.assertRaises(ValueError):module.bind_software_device('/private/dev/udmabuf')
if __name__=='__main__':unittest.main()

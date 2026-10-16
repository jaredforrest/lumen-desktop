import importlib.util,tempfile,unittest
from pathlib import Path
spec=importlib.util.spec_from_file_location('doctor',Path(__file__).resolve().parents[2]/'res/lumen-desktop/doctor.py');doctor=importlib.util.module_from_spec(spec);spec.loader.exec_module(doctor)
class PersistenceChecks(unittest.TestCase):
 def test_fast_storage_identity_is_recognized(self):
  with tempfile.TemporaryDirectory() as tmp:
   base=Path(tmp);old=base/'persist';fast=base/'persist-fast';user=base/'home/user/notifications'
   target=fast/str(user).lstrip('/');target.mkdir(parents=True);user.parent.mkdir(parents=True);user.symlink_to(target,target_is_directory=True)
   self.assertEqual(doctor.persistence_backing(user,(old,fast)),str(fast))
 def test_same_names_do_not_establish_persistence(self):
  with tempfile.TemporaryDirectory() as tmp:
   base=Path(tmp);user=base/'home/user/state';user.mkdir(parents=True);root=base/'persist';(root/str(user).lstrip('/')).mkdir(parents=True)
   self.assertIsNone(doctor.persistence_backing(user,(root,base/'missing')))
if __name__=='__main__':unittest.main()

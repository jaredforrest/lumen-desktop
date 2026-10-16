"""Account setup uses stdin and Secret Service; fixtures never contact iCloud."""
import json
from pathlib import Path
import sys,tempfile,unittest
from unittest.mock import patch
sys.path.insert(0,str(Path(__file__).resolve().parents[2]/'res/lumen-desktop/shell'))
import icloud_setup as setup
class AccountsTests(unittest.TestCase):
 def test_gui_password_is_keyring_stdin_only(self):
  with tempfile.TemporaryDirectory() as temp,patch('icloud_setup.shutil.which',return_value='/fixture/secret-tool'),patch('icloud_setup.subprocess.run') as run,patch('icloud_setup.run_sync'):
   root=Path(temp)/'calendar';settings=root/'icloud-settings'
   setup.setup_gui(root,settings,{'email':'fixture@example.test','password':'fixture-app-password'})
   argv=run.call_args.args[0]
   self.assertNotIn('fixture-app-password',' '.join(argv));self.assertEqual(run.call_args.kwargs['input'],'fixture-app-password')
   config=(settings/'vdirsyncer.conf').read_text();self.assertNotIn('fixture-app-password',config);self.assertIn('read_only = true',config)
 def test_malformed_requests_never_write_or_store(self):
  with tempfile.TemporaryDirectory() as temp,patch('icloud_setup.subprocess.run') as run:
   for request in ({'email':'bad','password':'pw'},{'email':'a@b','password':'\nsecret'},[],{'email':'a@b','password':''}):
    with self.assertRaises(ValueError):setup.setup_gui(Path(temp),Path(temp)/'settings',request)
   run.assert_not_called();self.assertFalse((Path(temp)/'settings').exists())
 def test_existing_account_cannot_silently_switch(self):
  with tempfile.TemporaryDirectory() as temp,patch('icloud_setup.shutil.which',return_value='/fixture/secret-tool'),patch('icloud_setup.subprocess.run') as run:
   root=Path(temp)/'calendar';settings=root/'icloud-settings';settings.mkdir(parents=True)
   config=settings/'vdirsyncer.conf';config.write_text(setup.configuration(root,settings,'one@example.test','/fixture/secret-tool'))
   before=config.read_bytes()
   with self.assertRaises(ValueError):setup.setup_gui(root,settings,{'email':'two@example.test','password':'fixture'})
   run.assert_not_called();self.assertEqual(config.read_bytes(),before)
if __name__=='__main__':unittest.main()

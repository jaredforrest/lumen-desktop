"""Observer projection and explicit setting writes, using no live providers."""
import importlib.util,sys,unittest
from pathlib import Path
from unittest.mock import patch
root=Path(__file__).resolve().parents[2]/'res/lumen-desktop'
sys.path[:0]=[str(root/'shell'),str(root/'runtime')]
from display_spaces import project
import display_space_settings as settings
from display_space_policy import identity_record

class Projection(unittest.TestCase):
 def setUp(self):
  self.outputs=[dict(name='A',active=True,model='Panel',make='',serial='')]
  self.native=dict(version=1,session='a'*32,generation='0',available=True,primary='A',separateDisplays=True)
  self.desired=dict(separateDisplays=True,primaryDisplay=None)
 def test_independent_default_and_null_primary(self):
  self.assertIsNone(identity_record(None));value=project(self.native,self.outputs,self.desired)
  self.assertTrue(value['applied']);self.assertTrue(value['independent']);self.assertIsNone(value['primaryDisplay'])
 def test_coupled_missing_registration_never_claims_independent_or_main(self):
  value=project(dict(self.native,available=False),self.outputs,dict(self.desired,separateDisplays=False))
  self.assertFalse(value['applied']);self.assertFalse(value['independent']);self.assertEqual(value['primary'],'');self.assertTrue(value['error'])
 def test_main_must_still_be_connected(self):
  value=project(dict(self.native,primary='gone'),self.outputs,self.desired)
  self.assertFalse(value['available']);self.assertEqual(value['primary'],'')
 def test_legacy_compositor_projects_explicit_usable_unapplied_fallback(self):
  value=project(None,self.outputs,dict(self.desired,separateDisplays=False))
  self.assertTrue(value['independent']);self.assertFalse(value['nativeSupported']);self.assertFalse(value['applied']);self.assertTrue(value['error'])
 def test_malformed_current_facts_never_become_applied(self):
  for change in [dict(version=True),dict(session='unknown'),dict(generation=1),dict(generation='01'),dict(generation=str(2**64)),dict(separateDisplays='false'),dict(available=1)]:
   with self.assertRaises(ValueError):project(dict(self.native,**change),self.outputs,self.desired)
 def test_changed_preferred_main_is_pending_until_native_matches(self):
  outputs=self.outputs+[dict(name='B',active=True,make='',model='Other',serial='',rect=dict(x=100,y=0))]
  desired=dict(self.desired,primaryDisplay='connector:B')
  value=project(self.native,outputs,desired);self.assertTrue(value['available']);self.assertFalse(value['applied']);self.assertTrue(value['error'])
  self.assertTrue(project(dict(self.native,primary='B'),outputs,desired)['applied'])
  self.assertFalse(project(self.native,outputs,self.desired,'connector:B')['applied'])
  self.assertTrue(project(self.native,self.outputs,desired)['applied'])
 def test_explicit_settings_update_preserves_other_field(self):
  with patch.object(settings,'configured',return_value=self.desired),patch.object(settings,'set_section') as write:
   self.assertTrue(settings.handle(dict(key='separateDisplays',value=False))['ok'])
   write.assert_called_once_with('spaces',dict(separateDisplays=False,primaryDisplay=None))
 def test_invalid_settings_never_dispatch(self):
  with patch.object(settings,'configured',return_value=self.desired),patch.object(settings,'set_section') as write:
   for data in [dict(key='separateDisplays',value=1),dict(key='unknown',value=True),dict(key='primaryDisplay',value={'kind':'connector','name':'A\n'})]:
    with self.assertRaises(ValueError):settings.handle(data)
   write.assert_not_called()
if __name__=='__main__':unittest.main()

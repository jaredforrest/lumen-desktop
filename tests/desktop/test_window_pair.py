"""Paired placement requests never substitute focus or a recycled partner."""
import importlib.util
from pathlib import Path
import unittest
from unittest.mock import patch
spec=importlib.util.spec_from_file_location('window_pair',Path(__file__).resolve().parents[2]/'res/lumen-desktop/shell/window_pair.py');module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)
class PairAuthority(unittest.TestCase):
 def setUp(self):
  self.commands=[]
  self.tree={'nodes':[dict(id=10,pid=100,app_id='a',name='First',type='con',lumen_pair={'eligible':True}),dict(id=20,pid=200,app_id='b',name='Second',type='con',lumen_pair={'eligible':True})]}
  def run(*args):
   self.commands.append(args)
   if args==('-t','get_version'):return {'native_paired_layouts':True}
   if args==('-t','get_tree'):return self.tree
   return [{'success':True}]
  self.fake=patch.object(module,'run',side_effect=run);self.fake.start();self.addCleanup(self.fake.stop)
 def test_explicit_right_source_and_partner_identity(self):
  self.assertEqual(module.handle(dict(action='create',id=10,pid=100,partner=20,partnerPid=200,side='right')),{'ok':True})
  self.assertEqual(self.commands[-1],('lumen_pair create 20 10',))
 def test_legacy_window_uses_class_without_changing_authority(self):
  self.tree['nodes'][0]['app_id']=None;self.tree['nodes'][0]['window_properties']={'class':'LegacyEditor'}
  status=module.handle(dict(action='status',id=10));self.assertEqual(status['source']['app'],'LegacyEditor')
  module.handle(dict(action='create',id=10,pid=100,partner=20,partnerPid=200,side='left'))
  self.assertEqual(self.commands[-1],('lumen_pair create 10 20',))
 def test_closed_source_never_substitutes_existing_window(self):
  self.tree['nodes'].pop(0)
  with self.assertRaisesRegex(ValueError,'captured window'):module.handle(dict(action='leave',id=10,pid=100))
  self.assertTrue(all(call[0]=='-t' for call in self.commands))
 def test_replaced_process_or_partner_rejects_before_action(self):
  for request in (dict(action='swap',id=10,pid=101),dict(action='replace',id=10,pid=100,partner=20,partnerPid=201)):
   with self.assertRaises(ValueError):module.handle(request)
  self.assertTrue(all(call[0]=='-t' for call in self.commands))
 def test_numeric_identity_and_ratio_are_not_command_text(self):
  for value in (True,0,-1,'10; exit',2**53):
   with self.assertRaises(ValueError):module.handle(dict(action='leave',id=value,pid=100))
  for value in (float('nan'),float('inf'),True,0,1,'0.5; exit'):
   with self.assertRaises(ValueError):module.handle(dict(action='ratio',id=10,pid=100,ratio=value))
  self.assertTrue(all(call[0]=='-t' for call in self.commands))
 def test_status_hides_existing_pairs_and_own_picker(self):
  self.tree['nodes'][1]['lumen_pair']['id']=30
  self.tree['nodes'].append(dict(id=40,pid=400,app_id='org.quickshell',name='Paired Windows',type='con',lumen_pair={'eligible':True}))
  self.assertEqual(module.handle(dict(action='status',id=10))['candidates'],[])
if __name__=='__main__':unittest.main()

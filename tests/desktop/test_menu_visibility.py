"""Authoritative fullscreen state is scoped to active workspaces and outputs."""
import importlib.util,unittest
from pathlib import Path
spec=importlib.util.spec_from_file_location('menu_visibility',Path(__file__).resolve().parents[2]/'res/lumen-desktop/shell/menu_visibility.py');m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m)
class Visibility(unittest.TestCase):
 def output(self,name,current,**extra):return dict(name=name,current_workspace=current,active=True,**extra)
 def tree(self,mode=1):return dict(nodes=[dict(type='output',name='one',nodes=[dict(type='workspace',name='active',fullscreen_mode=1,nodes=[dict(type='con',fullscreen_mode=mode)]),dict(type='workspace',name='hidden',nodes=[dict(type='con',fullscreen_mode=1)])]),dict(type='output',name='two',nodes=[dict(type='workspace',name='other',fullscreen_mode=1,nodes=[])])])
 def test_local_active_workspace(self):self.assertEqual(m.snapshot(self.tree(),[self.output('one','active'),self.output('two','other')]),{'one':True,'two':False})
 def test_dormant_fullscreen_not_visible(self):self.assertEqual(m.snapshot(self.tree(0),[self.output('one','active')]),{'one':False})
 def test_global_fullscreen(self):self.assertEqual(m.snapshot(self.tree(2),[self.output('one','active'),self.output('two','other')]),{'one':True,'two':True})
 def test_removed_output_and_invalid_state(self):
  self.assertEqual(m.snapshot(self.tree(),[self.output('two','other')]),{'two':False})
  with self.assertRaises(ValueError):m.snapshot(self.tree(True),[self.output('one','active')])
if __name__=='__main__':unittest.main()

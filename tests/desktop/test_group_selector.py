"""Authoritative recent-group projection and captured selector scope."""
import copy
import importlib.util
from pathlib import Path
import sys
import unittest
from unittest.mock import patch
SHELL=Path(__file__).resolve().parents[2]/'res/lumen-desktop/shell'
sys.path.insert(0,str(SHELL))
spec=importlib.util.spec_from_file_location('group_selector',SHELL/'window_group_selector.py')
selector=importlib.util.module_from_spec(spec);spec.loader.exec_module(selector)

class SelectorTests(unittest.TestCase):
 def setUp(self):
  self.groups=dict(version=1,revision=8,enabled=True,locked=False,separate_displays=True,groups=[dict(id=i,workspace=w,selected=i in (1,3,4),recent=r,suspended=False,members=[dict(id=100+i,pid=500+i,group_hidden=i==2)]) for i,w,r in ((1,10,1),(2,10,9),(3,11,15),(4,20,30))])
  self.outputs=[dict(name='A',active=True,focused=True),dict(name='B',active=True)]
  self.workspaces=[dict(id=10,output='A',visible=True),dict(id=11,output='A',visible=False),dict(id=20,output='B',visible=True)]
  self.version=dict(native_window_group_selector=True,lumen_session_lock=dict(version=1,available=True,locked=False))
 def project(self):return selector.project(dict(window_groups=self.groups),self.outputs,self.workspaces,self.version,'epoch')
 def test_recent_order_is_scoped_to_visible_workspace_and_named_output(self):
  value=self.project();self.assertEqual([g['id'] for g in value['outputs'][0]['groups']],[2,1]);self.assertEqual([g['id'] for g in value['outputs'][1]['groups']],[4]);self.assertEqual(value['focusedOutput'],'A')
 def test_disconnected_output_removed_without_retargeting(self):
  self.outputs[0]['active']=False;self.assertEqual([o['name'] for o in self.project()['outputs']],['B'])
 def test_unknown_capability_or_lock_facts_fail_closed(self):
  for version in ({},dict(self.version,native_window_group_selector=1),dict(self.version,lumen_session_lock=dict(version=True,available=True,locked=False))):
   with self.assertRaises(ValueError):selector.project(dict(window_groups=self.groups),self.outputs,self.workspaces,version,'epoch')
 def test_either_lock_sample_closes_selection(self):
  self.version['lumen_session_lock']['locked']=True;self.assertTrue(self.project()['locked'])
 def test_native_scoped_command_uses_captured_ids_and_epoch(self):
  native=selector.Native();native.path='fixture';native.epoch='epoch';state=self.project();sent=[]
  native.snapshot=lambda:state;native.query=lambda kind,value:sent.append((kind,value)) or [dict(success=True)]
  with patch.object(selector,'runtime_epoch',return_value='epoch'):
   self.assertEqual(native.activate(dict(epoch='epoch',revision=8,output='A',workspace=10,id=2)),dict(ok=True))
  self.assertEqual(sent,[(0,'lumen_window groups 8 select-scoped 2 10 "A"')])
 def test_stale_epoch_revision_hidden_workspace_wrong_output_and_lock_rejected(self):
  native=selector.Native();native.path='fixture';native.epoch='epoch';native.snapshot=self.project;native.query=lambda *args:self.fail('Unexpected mutation')
  request=dict(epoch='epoch',revision=8,output='A',workspace=10,id=2)
  for change in (dict(epoch='old'),dict(revision=7),dict(revision=True),dict(workspace=11,id=3),dict(output='B'),dict(id=4),dict(id=True)):
   with self.assertRaises(ValueError):native.activate(dict(request,**change))
  self.groups['locked']=True
  with self.assertRaises(ValueError):native.activate(request)
 def test_unknown_or_ambiguous_visible_workspace_not_exposed(self):
  self.workspaces[1]['visible']=True;self.assertEqual([o['name'] for o in self.project()['outputs']],['B'])

if __name__=='__main__':unittest.main()

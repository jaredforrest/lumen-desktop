"""Strict native facts and captured group operations; no compositor effects."""
import copy
import importlib.util
from unittest.mock import patch
from pathlib import Path
import sys
import unittest
sys.path.insert(0,str(Path(__file__).resolve().parents[2]/'res/lumen-desktop/runtime'))
from window_groups import Groups

class GroupTests(unittest.TestCase):
 def setUp(self):
  self.state=dict(version=1,revision=7,enabled=True,locked=False,separate_displays=True,groups=[dict(id=1,workspace=10,selected=True,recent=2,suspended=False,members=[dict(id=101,pid=501,group_hidden=False)]),dict(id=2,workspace=10,selected=False,recent=1,suspended=False,members=[dict(id=102,pid=502,group_hidden=True)])])
  self.request=dict(groupRevision=7,groupId=1,id=102,pid=502,groupAction='add')
 def test_exact_command_has_only_validated_numeric_arguments(self):
  self.assertEqual(Groups.read(self.state).command(self.request),'lumen_window groups 7 add 1 102 502')
 def test_unknown_old_compositor_fails_closed(self):
  for value in (None,{},dict(self.state,version=True)):
   with self.assertRaises(ValueError):Groups.read(value)
 def test_coupled_groups_retained_but_actions_suspended(self):
  groups=Groups.read(dict(self.state,separate_displays=False))
  self.assertEqual(len(groups.groups),2)
  for action in ('on','select','add','remove'):
   with self.assertRaises(ValueError):groups.command(dict(self.request,groupAction=action))
  self.assertEqual(groups.command(dict(self.request,groupAction='off')),'lumen_window groups 7 off')
 def test_bool_ids_and_injection_rejected(self):
  for value in (True,0,-1,'1; kill',2**64):
   with self.assertRaises(ValueError):Groups.read(self.state).command(dict(self.request,id=value))
 def test_stale_revision_and_pid_rejected(self):
  for change in (dict(groupRevision=6),dict(groupRevision=True),dict(pid=999),dict(groupId=999)):
   with self.assertRaises(ValueError):Groups.read(self.state).command(dict(self.request,**change))
 def test_locked_or_suspended_mutations_rejected(self):
  value=copy.deepcopy(self.state);value['locked']=True
  with self.assertRaises(ValueError):Groups.read(value).command(self.request)
  value['locked']=False;value['groups'][0]['suspended']=True
  with self.assertRaises(ValueError):Groups.read(value).command(self.request)
 def test_duplicate_memberships_or_selected_groups_rejected(self):
  for field,value in [('members',self.state['groups'][0]['members']),('selected',True)]:
   state=copy.deepcopy(self.state);state['groups'][1][field]=value
   with self.assertRaises(ValueError):Groups.read(state)
 def test_remove_requires_exact_group_membership(self):
  with self.assertRaises(ValueError):Groups.read(self.state).command(dict(self.request,groupAction='remove'))
 def test_overview_rejects_previous_session_before_command(self):
  path=Path(__file__).resolve().parents[2]/'res/lumen-desktop/shell/window_overview.py'
  spec=importlib.util.spec_from_file_location('group_overview',path);module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)
  with patch.object(module,'endpoint',return_value='new-session'),patch.object(module,'snapshot',return_value={'windowGroups':self.state}),patch.object(module,'checked') as checked:
   with self.assertRaisesRegex(ValueError,'session changed'):
    module.handle(dict(self.request,action='window-groups',sessionEpoch='old-session'))
   checked.assert_not_called()
 def test_off_is_independent_of_suspended_group_and_contains_no_restore(self):
  self.state['groups'][0]['suspended']=True
  self.assertEqual(Groups.read(self.state).command(dict(groupRevision=7,groupAction='off')),'lumen_window groups 7 off')

if __name__=='__main__':unittest.main()

"""Captured membership intents never retarget source/group/desktop on a refresh."""
import importlib.util
from pathlib import Path
import sys
import unittest
SHELL=Path(__file__).resolve().parents[2]/'res/lumen-desktop/shell';sys.path.insert(0,str(SHELL))
spec=importlib.util.spec_from_file_location('membership',SHELL/'window_group_membership.py');module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)
from window_groups import Groups

class MembershipTests(unittest.TestCase):
 def setUp(self):
  self.groups=Groups.read(dict(version=1,revision=8,enabled=True,locked=False,separate_displays=True,groups=[dict(id=i,workspace=10 if i<3 else 20,selected=i in (1,3),recent=i,suspended=False,members=[dict(id=100+i,pid=500+i,group_hidden=i==2)]) for i in (1,2,3)]))
  self.state=dict(epoch='epoch',revision=8,enabled=True,locked=False,outputs=[dict(name='A',workspace=10,suspended=False),dict(name='B',workspace=20,suspended=False)])
  self.request=dict(epoch='epoch',revision=8,sourceGroup=1,workspace=10,output='A',id=101,pid=501,kind='group',groupId=2)
 def test_add_has_exact_source_and_visible_scope_in_atomic_native_command(self):
  self.assertEqual(module.command(self.state,self.groups,self.request),'lumen_window groups 8 add-scoped 2 10 "A" 101 501 1')
 def test_detach_to_new_group_has_exact_source_identity_and_no_placement_command(self):
  self.assertEqual(module.command(self.state,self.groups,dict(self.request,kind='new')),'lumen_window groups 8 remove-scoped 1 10 "A" 101 501')
 def test_stale_source_session_revision_pid_or_group_rejected(self):
  for change in (dict(epoch='old'),dict(revision=7),dict(revision=True),dict(sourceGroup=2),dict(pid=502),dict(id=True),dict(groupId=1),dict(groupId=3),dict(kind='implicit')):
   with self.assertRaises(ValueError):module.command(self.state,self.groups,dict(self.request,**change))
 def test_output_workspace_lock_or_revision_race_rejected_without_retarget(self):
  for change in (dict(output='B'),dict(workspace=20)):
   with self.assertRaises(ValueError):module.command(self.state,self.groups,dict(self.request,**change))
  for change in (dict(locked=True),dict(enabled=False),dict(revision=9)):
   with self.assertRaises(ValueError):module.command(dict(self.state,**change),self.groups,self.request)

if __name__=='__main__':unittest.main()

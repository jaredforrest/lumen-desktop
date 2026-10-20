"""Managed membership ownership and conservative native eligibility."""
import os,sys,tempfile,unittest
from pathlib import Path
from unittest.mock import patch
sys.path.insert(0,str(Path(__file__).resolve().parents[2]/'res/lumen-desktop/runtime'))
from spaces import SpacePolicy
import application_desktops as desktop

class MembershipTests(unittest.TestCase):
 def setUp(self):
  self.tmp=tempfile.TemporaryDirectory();self.addCleanup(self.tmp.cleanup)
  self.policy=SpacePolicy(Path(self.tmp.name)/'policy.json');self.policy.assign('wayland:editor','all')
  self.node=dict(id=42,pid=os.getpid(),app_id='editor',sticky=False,lumen_relationships=dict(parent_supported=True,parent=None,modal=None))
 def tree(self,node=None):return dict(nodes=[dict(type='workspace',name='1',floating_nodes=[node or self.node])])
 def reconcile(self,tree=None,epoch='session'):
  with patch.object(desktop,'runtime_epoch',return_value=epoch),patch.dict(os.environ,SWAYSOCK='private'),patch.object(desktop,'instance',wraps=desktop.instance):
   commands=[];desktop.reconcile(self.policy,tree or self.tree(),commands.append);return commands
 def test_restart_clear_only_managed(self):
  self.assertEqual(self.reconcile(),['[con_id=42] sticky enable'])
  self.node['sticky']=True;self.policy=SpacePolicy(self.policy.path)
  self.assertEqual(self.reconcile(),[])
  self.policy.assign('wayland:editor','none')
  self.assertEqual(self.reconcile(),['[con_id=42] sticky disable'])
 def test_preexisting_sticky_and_reused_identity_never_undone(self):
  self.node['sticky']=True;self.assertEqual(self.reconcile(),[])
  self.policy.assign('wayland:editor','none');self.assertEqual(self.reconcile(),[])
  self.node['sticky']=False;self.policy.assign('wayland:editor','all');self.reconcile()
  self.node['sticky']=True;self.policy.assign('wayland:editor','none')
  self.assertEqual(self.reconcile(epoch='new-session'),[])
  self.assertFalse(self.policy.state['managed_sticky'])
 def test_reused_process_birth_never_undone(self):
  self.reconcile();self.node['sticky']=True;self.policy.assign('wayland:editor','none')
  self.policy.state['managed_sticky']['42']['start']='different'
  self.assertEqual(self.reconcile(),[])
 def test_unsupported_native_states_unchanged(self):
  for changes in [dict(_floating=False),dict(_grouped=True),dict(fullscreen_mode=1),dict(lumen_pair=dict(id=4)),dict(lumen_relationships=dict(parent_supported=False)),dict(lumen_relationships=dict(parent_supported=True,parent=9)),dict(lumen_relationships=dict(parent_supported=True,parent=None,modal=True)),dict(lumen_state=dict(filled=True)),dict(lumen_state=dict(minimised=True))]:
   n=dict(self.node,_floating=True,_grouped=False);n.update(changes)
   self.assertIsNotNone(desktop.reason(n),changes)
 def test_tree_ancestors_and_group_identity(self):
  tree=self.tree();tree['window_groups']=dict(groups=[dict(members=[dict(id=42)])])
  n=desktop.windows(tree)[0];self.assertTrue(n['_floating']);self.assertTrue(n['_grouped']);self.assertEqual(n['_workspace'],'1')
 def test_floating_split_descendant_does_not_gain_sibling_membership(self):
  tree=dict(nodes=[dict(type='workspace',name='1',floating_nodes=[dict(id=7,nodes=[self.node])])])
  node=desktop.windows(tree)[0];self.assertFalse(node['_floating']);self.assertIn('Tiled',desktop.reason(node))
  self.assertEqual(self.reconcile(tree),[])
 def test_assignment_change_restores_owned_sticky_without_moving(self):
  self.reconcile();self.node['sticky']=True;self.policy.assign('wayland:editor','display','connector:B')
  self.assertEqual(self.reconcile(),['[con_id=42] sticky disable'])

if __name__=='__main__':unittest.main()

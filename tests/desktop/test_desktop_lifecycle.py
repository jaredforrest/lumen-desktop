"""Logical desktop durability, exact order and evacuation safety; no live IPC."""
import copy
import importlib.util
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch
RUNTIME=Path(__file__).resolve().parents[2]/'res/lumen-desktop/runtime'
sys.path.insert(0,str(RUNTIME))
from spaces import SpacePolicy
spec=importlib.util.spec_from_file_location('overview_lifecycle',RUNTIME.parent/'shell/window_overview.py')
overview=importlib.util.module_from_spec(spec);spec.loader.exec_module(overview)


class DesktopPolicyTests(unittest.TestCase):
    def setUp(self):
        self.temp=tempfile.TemporaryDirectory();self.addCleanup(self.temp.cleanup)
        self.policy=SpacePolicy(Path(self.temp.name)/'spaces.json')
        self.outputs=[dict(name='A',active=True,focused=True),dict(name='B',active=True)]
        self.workspaces=[dict(id=1,name='1',output='A',focused=True),dict(id=2,name='2',output='A'),dict(id=3,name='3',output='B')]
        self.policy.reconcile(self.workspaces,self.outputs)
        self.keys=dict(self.policy.current)

    def test_empty_desktop_survives_restart_without_forcing_native_focus(self):
        desktop=self.policy.create('connector:A',self.policy.state['revision'])
        self.assertIsNone(desktop['id']);self.assertFalse(desktop['focused'])
        other=SpacePolicy(self.policy.path);other.reconcile(self.workspaces,self.outputs)
        self.assertEqual(other.desktop(desktop['logicalId'])['name'],desktop['name'])
        self.assertEqual(len(other.catalog()),4)

    def test_reorder_disables_recent_order_without_renaming_or_reidentifying(self):
        before=copy.deepcopy(self.policy.state['desktops'])
        self.policy.focus(1);self.policy.reorder('connector:A',[self.keys[2],self.keys[1]],self.policy.state['revision'])
        self.assertFalse(self.policy.state['auto_reorder']);self.assertEqual(self.policy.order('A'),[self.keys[2],self.keys[1]])
        self.assertEqual(self.policy.state['desktops'][self.keys[3]],before[self.keys[3]])
        self.assertEqual({key:item['name'] for key,item in self.policy.state['desktops'].items()},{key:item['name'] for key,item in before.items()})
        self.policy.focus(1);self.assertEqual(self.policy.order('A'),[self.keys[2],self.keys[1]])
        self.policy.configure(auto_reorder=True);self.assertEqual(self.policy.order('A')[0],self.keys[1])

    def test_stale_revision_duplicate_cross_output_and_incomplete_orders_rejected(self):
        revision=self.policy.state['revision']
        for order in ([self.keys[1]],[self.keys[1],self.keys[1]],[self.keys[1],self.keys[3]]):
            with self.assertRaises(ValueError):self.policy.reorder('connector:A',order,revision)
        self.policy.focus(2)
        with self.assertRaisesRegex(ValueError,'changed'):self.policy.create('connector:A',revision)
        with self.assertRaises(ValueError):self.policy.create('disconnected',self.policy.state['revision'])

    def test_output_disconnect_rehomes_dormant_identity_to_reachable_output(self):
        desktop=self.policy.create('connector:B',self.policy.state['revision'])
        self.policy.reconcile(self.workspaces[:2],self.outputs[:1])
        value=self.policy.desktop(desktop['logicalId']);self.assertEqual(value['output'],'A');self.assertTrue(value['available'])
        self.policy.reconcile([],[])
        self.assertFalse(self.policy.desktop(desktop['logicalId'],connected=False)['available'])
        with self.assertRaisesRegex(ValueError,'disconnected'):self.policy.desktop(desktop['logicalId'])

    def test_reconcile_noop_does_not_invalidate_gesture_revision(self):
        revision=self.policy.state['revision'];self.policy.reconcile(self.workspaces,self.outputs)
        self.assertEqual(revision,self.policy.state['revision'])

    def test_retirement_requires_evacuated_source_and_same_output_survivor(self):
        revision=self.policy.state['revision']
        with self.assertRaisesRegex(ValueError,'still exists'):self.policy.remove(self.keys[1],self.keys[2],revision)
        with self.assertRaisesRegex(ValueError,'same display'):self.policy.remove(self.keys[1],self.keys[3],revision)
        self.policy.reconcile(self.workspaces[1:],self.outputs)
        self.policy.remove(self.keys[1],self.keys[2],self.policy.state['revision'])
        self.assertNotIn(self.keys[1],self.policy.state['desktops'])


class EvacuationTests(unittest.TestCase):
    def setUp(self):
        DesktopPolicyTests.setUp(self)
        self.windows=[dict(id=10,pid=100,workspace='1',restores=['1'],fullscreen=0,paired=False),
                      dict(id=11,pid=101,workspace='__i3_scratch',restores=['1'],fullscreen=0,paired=False),
                      dict(id=12,pid=102,workspace='2',restores=[],fullscreen=0,paired=False)]
        self.native=copy.deepcopy(self.workspaces);self.commands=[]
        self.data=dict(source=self.keys[1],destination=self.keys[2],revision=self.policy.state['revision'],sessionEpoch='session')
    def snapshot(self,**kwargs):return dict(windows=copy.deepcopy(self.windows),workspaces=copy.deepcopy(self.native),outputs=copy.deepcopy(self.outputs))
    def fake_run(self,*args):
        if args==('-t','get_outputs'):return self.outputs
        if args==('-t','get_version'):return dict(desktop_rehome=True)
        command=args[0];self.commands.append(command)
        if command.startswith('[con_id='):
            identifier=int(command.split(']')[0].split('=')[1]);window=next(window for window in self.windows if window['id']==identifier)
            if 'lumen_window rehome' in command:window['restores']=['2' if name=='1' else name for name in window['restores']]
            elif 'move container to workspace' in command:window['workspace']='2'
        elif 'workspace "2"' in command:
            for workspace in self.native:workspace['focused']=workspace['name']=='2'
            self.native=[workspace for workspace in self.native if workspace['name']!='1']
        return [dict(success=True)]
    def execute(self):
        with patch.object(overview,'endpoint',return_value='session'),patch.object(overview,'snapshot',side_effect=self.snapshot),patch.object(overview,'run',side_effect=self.fake_run):
            return overview.remove_desktop(self.policy,self.data,'session')
    def test_visible_and_hidden_saved_placements_migrate_without_unhide_or_close(self):
        result=self.execute();self.assertTrue(result['ok'])
        self.assertEqual(self.windows[0]['workspace'],'2');self.assertEqual(self.windows[1]['workspace'],'__i3_scratch')
        self.assertEqual(self.windows[1]['restores'],['2']);self.assertEqual(self.windows[2]['workspace'],'2')
        self.assertNotIn(self.keys[1],self.policy.state['desktops'])
        self.assertFalse(any(word in command for command in self.commands for word in ('kill','close','scratchpad show','unhide')))
    def test_new_arrival_aborts_retirement_and_preserves_moved_windows(self):
        original=self.snapshot;count=[0]
        def snapshot(**kwargs):
            count[0]+=1
            if count[0]==4:self.windows.append(dict(id=13,pid=103,workspace='1',restores=[],fullscreen=0,paired=False))
            return original(**kwargs)
        self.snapshot=snapshot
        with self.assertRaisesRegex(ValueError,'changed during removal'):self.execute()
        self.assertIn(self.keys[1],self.policy.state['desktops']);self.assertEqual(self.windows[0]['workspace'],'2')
    def test_changed_session_fails_before_any_window_mutation(self):
        self.data['sessionEpoch']='old'
        with self.assertRaisesRegex(ValueError,'session changed'):self.execute()
        self.assertEqual(self.commands,[])
    def test_older_compositor_fails_closed_before_any_window_mutation(self):
        original=self.fake_run
        def run(*args):return {} if args==('-t','get_version') else original(*args)
        self.fake_run=run
        with self.assertRaisesRegex(ValueError,'all saved window placements'):self.execute()
        self.assertEqual(self.commands,[])

    def test_paired_source_is_explicitly_rejected_without_mutation(self):
        self.windows[0]['paired']=True
        with self.assertRaisesRegex(ValueError,'paired'):self.execute()
        self.assertEqual(self.commands,[])


if __name__=='__main__':unittest.main()

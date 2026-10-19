"""Adversarial runtime fixtures; no live compositor, clipboard or user state."""
import importlib.util
import json
import math
import os
from pathlib import Path
import sys
import tempfile
import unittest

RUNTIME=Path(__file__).resolve().parents[2]/'res/lumen-desktop/runtime'
sys.path.insert(0,str(RUNTIME))
import clipboard_history
import protocol
import state


def compositor_fixture():
    rect=dict(x=0,y=0,width=800,height=600)
    window=dict(type='con',id=7,app_id='org.example.Fixture',focused=True,rect=rect,border='csd',nodes=[],floating_nodes=[])
    workspace=dict(type='workspace',id=3,name='fixture',nodes=[window],floating_nodes=[])
    tree=dict(type='root',nodes=[dict(type='output',name='FIXTURE-1',nodes=[workspace],floating_nodes=[])],floating_nodes=[])
    outputs=[dict(name='FIXTURE-1',active=True,focused=True,rect=rect,scale=1)]
    return tree,outputs,window,workspace


class ClipboardValidation(unittest.TestCase):
    def setUp(self):
        self.temp=tempfile.TemporaryDirectory();self.root=Path(self.temp.name);self.now=[10.0]
        self.history=clipboard_history.History(self.root/'runtime',self.root/'state',retention=60,clock=lambda:self.now[0],wall=lambda:123.0)
    def tearDown(self):self.temp.cleanup()

    def test_false_empty_malformed_and_unknown_consent_never_enable_collection(self):
        for text in ('', '{}','{"version":1,"enabled":false}','{"version":2,"enabled":true}','not json'):
            with self.subTest(text=text):
                self.history.consent.write_text(text)
                self.assertFalse(self.history.enabled())
                self.assertFalse(self.history.ingest('synthetic fixture')['recorded'])

    def test_expired_text_is_removed_from_runtime_storage(self):
        self.history.set_enabled(True);self.history.ingest('synthetic expiring text')
        self.now[0]=71.0;self.history.prune()
        self.assertEqual(json.loads(self.history.data.read_text()),[])
        self.assertNotIn('synthetic expiring text',self.history.data.read_text())

    def test_malformed_history_rows_fail_closed(self):
        self.history.set_enabled(True);self.history.ingest('synthetic row')
        valid=json.loads(self.history.data.read_text())[0]
        invalid=[None,False,{},dict(valid,text=7),dict(valid,id=[]),dict(valid,created_monotonic='10'),dict(valid,created_monotonic=True),dict(valid,created_monotonic=math.nan),dict(valid,timestamp=math.inf)]
        for row in invalid:
            with self.subTest(row_type=type(row).__name__):
                self.history.data.write_text(json.dumps([row]))
                with self.assertRaises((ValueError,TypeError,KeyError)):
                    self.history.list()
                copied=[]
                with self.assertRaises((ValueError,TypeError,KeyError)):
                    self.history.copy(valid['id'],copied.append)
                self.assertEqual(copied,[])

    def test_symlinked_consent_cannot_enable_history(self):
        target=self.root/'unrelated';target.write_text('{"version":1,"enabled":true}')
        self.history.consent.symlink_to(target)
        self.assertFalse(self.history.enabled())
        self.assertFalse(self.history.ingest('synthetic fixture')['recorded'])
        self.assertEqual(target.read_text(),'{"version":1,"enabled":true}')


class CursorHighWater(unittest.TestCase):
    def test_gap_requires_snapshot_at_or_after_observed_future_event(self):
        cursor=protocol.RevisionCursor();cursor.snapshot('fixture',1,1)
        with self.assertRaises(protocol.ProtocolError):cursor.event('fixture',4,4)
        for sequence,revision in ((1,1),(3,4),(4,3)):
            with self.subTest(sequence=sequence,revision=revision),self.assertRaises(protocol.ProtocolError):
                cursor.snapshot('fixture',sequence,revision)
            self.assertFalse(cursor.ready)
        cursor.snapshot('fixture',4,4);self.assertTrue(cursor.ready)
        cursor.event('fixture',5,4)


class StateValidation(unittest.TestCase):
    def test_desktop_reveal_is_independent_from_app_hiding_and_minimisation(self):
        tree,outputs,window,_=compositor_fixture()
        window['lumen_state']={'minimised':False,'filled':False,'hidden':False,'desktop_hidden':True}
        observed=state.snapshot(tree,outputs,'fixture')['windows'][0]
        self.assertTrue(observed['desktop_hidden'])
        self.assertFalse(observed['app_hidden'])
        self.assertFalse(observed['minimised'])
        window['lumen_state']['desktop_hidden']='false'
        with self.assertRaises(protocol.ProtocolError):state.snapshot(tree,outputs,'fixture')

    def test_malformed_children_and_window_properties_are_rejected(self):
        for field,value in (('nodes',5),('floating_nodes',{}),('window_properties','invalid')):
            tree,outputs,window,_=compositor_fixture();window[field]=value
            with self.subTest(field=field),self.assertRaises(protocol.ProtocolError):
                state.snapshot(tree,outputs,'fixture')

    def test_false_string_cannot_assert_keyboard_focus(self):
        tree,outputs,window,_=compositor_fixture();window['focused']='false'
        with self.assertRaises(protocol.ProtocolError):state.snapshot(tree,outputs,'fixture')

    def test_invalid_app_identity_is_rejected(self):
        for identity in (False,[],{'private':'not an identity'}):
            tree,outputs,window,_=compositor_fixture();window['app_id']=identity
            with self.subTest(identity_type=type(identity).__name__),self.assertRaises(protocol.ProtocolError):
                state.snapshot(tree,outputs,'fixture')

    def test_invalid_scale_and_duplicate_output_are_rejected(self):
        for scale in (0,-1,True,'1',math.inf):
            tree,outputs,_,_=compositor_fixture();outputs[0]['scale']=scale
            with self.subTest(scale=scale),self.assertRaises(protocol.ProtocolError):
                state.snapshot(tree,outputs,'fixture')
        tree,outputs,_,_=compositor_fixture();outputs.append(dict(outputs[0]))
        with self.assertRaises(protocol.ProtocolError):state.snapshot(tree,outputs,'fixture')

    def test_duplicate_workspace_identity_is_rejected(self):
        tree,outputs,_,workspace=compositor_fixture()
        tree['nodes'][0]['nodes'].append({**workspace,'nodes':[]})
        with self.assertRaises(protocol.ProtocolError):state.snapshot(tree,outputs,'fixture')


if __name__=='__main__':unittest.main()

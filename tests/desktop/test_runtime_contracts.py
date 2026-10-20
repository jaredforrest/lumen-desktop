"""Framing, uncertain-command replay and compositor observation regressions."""
import copy
import importlib.util
import math
import socket
import struct
import sys
import tempfile
import unittest
from pathlib import Path

RUNTIME=Path(__file__).resolve().parents[2]/'res/lumen-desktop/runtime'
sys.path.insert(0,str(RUNTIME))
import protocol
import state


class Framing(unittest.TestCase):
    def test_fragmentation_and_multiple_frames(self):
        wire=protocol.encode({'text':'café'})+protocol.encode({'second':True})
        decoder=protocol.Decoder();values=[]
        for byte in wire:values.extend(decoder.feed(bytes([byte])))
        decoder.finish()
        self.assertEqual(values,[{'text':'café'},{'second':True}])
    def test_duplicates_in_nested_objects_rejected(self):
        with self.assertRaises(protocol.ProtocolError):protocol.loads('{"args":{"id":1,"id":2}}')
    def test_nonfinite_and_exponent_overflow_rejected(self):
        for raw in ('NaN','Infinity','-Infinity','1e10000','{"x":[NaN]}'):
            with self.subTest(raw=raw),self.assertRaises(protocol.ProtocolError):protocol.loads(raw)
        with self.assertRaises(protocol.ProtocolError):protocol.encode({'x':math.nan})
    def test_ambiguous_encoder_keys_rejected(self):
        with self.assertRaises(protocol.ProtocolError):protocol.encode({1:'one','1':'other'})
    def test_invalid_utf8(self):
        with self.assertRaises(protocol.ProtocolError):protocol.loads(b'"\xff"')
    def test_oversize_rejected_before_body_allocation(self):
        decoder=protocol.Decoder()
        with self.assertRaises(protocol.ProtocolError):decoder.feed(struct.pack('!I',protocol.MAX_FRAME+1))
        self.assertEqual(len(decoder.buffer),0)
        with self.assertRaises(protocol.ProtocolError):decoder.feed(protocol.encode({}))
    def test_truncated_body(self):
        decoder=protocol.Decoder();decoder.feed(protocol.encode({'x':1})[:-1])
        with self.assertRaises(protocol.ProtocolError):decoder.finish()
    def test_empty_frame(self):
        with self.assertRaises(protocol.ProtocolError):protocol.Decoder().feed(b'\0\0\0\0')


class CommandReconciliation(unittest.TestCase):
    def test_lost_ack_retry_retains_identity(self):
        clock=[1];guard=protocol.ReplayGuard(lambda:clock[0],capacity=1)
        self.assertTrue(guard.remember('epoch','peer','request',{'action':'close'}))
        self.assertFalse(guard.remember('epoch','peer','request',{'action':'close'}))
        with self.assertRaises(protocol.ProtocolError):guard.remember('epoch','peer','request',{'action':'quit'})
        with self.assertRaises(protocol.ProtocolError):guard.remember('epoch','peer','other',{})
        clock[0]=602
        self.assertTrue(guard.remember('epoch','peer','other',{}))
    def test_same_request_id_is_scoped_to_peer_and_epoch(self):
        guard=protocol.ReplayGuard(lambda:1)
        for epoch,peer in [('one','A'),('one','B'),('two','A')]:
            self.assertTrue(guard.remember(epoch,peer,'id',{}))
    def test_sequence_gap_requires_reconciliation(self):
        cursor=protocol.RevisionCursor();cursor.snapshot('one',4,2);cursor.event('one',5,2)
        with self.assertRaises(protocol.ProtocolError):cursor.event('one',7,3)
        self.assertFalse(cursor.ready)
        cursor.snapshot('one',7,3);cursor.event('one',8,4)
        with self.assertRaises(protocol.ProtocolError):cursor.snapshot('one',6,3)
    def test_epoch_change_does_not_reuse_ids(self):
        cursor=protocol.RevisionCursor();cursor.snapshot('one',3,1)
        with self.assertRaises(protocol.ProtocolError):cursor.event('two',4,2)
        cursor.snapshot('two',4,2);self.assertTrue(cursor.ready)
    def test_boolean_revision_is_not_integer(self):
        with self.assertRaises(protocol.ProtocolError):protocol.RevisionCursor().snapshot('one',True,1)


RECT={'x':10,'y':20,'width':800,'height':600}
def fixture():
    window={'id':7,'type':'con','app_id':'org.example.Editor','name':'DO NOT EXPORT PRIVATE DOCUMENT','pid':123,'focused':True,'rect':dict(RECT),'border':'csd','nodes':[],'floating_nodes':[]}
    workspace={'id':3,'type':'workspace','name':'__i3_scratch','nodes':[],'floating_nodes':[window]}
    tree={'type':'root','nodes':[{'type':'output','name':'TEST-1','nodes':[workspace]}]}
    outputs=[{'name':'TEST-1','active':True,'focused':True,'rect':dict(RECT),'scale':1.5}]
    return tree,outputs,window


class CompositorObservation(unittest.TestCase):
    def test_preferred_zoom_is_explicit_native_metadata(self):
        tree,outputs,window=fixture()
        window['lumen_state']=dict(minimised=False,filled=True,zoomed=True,preferred_zoom=True)
        report=state.snapshot(tree,outputs,'epoch')
        self.assertEqual(report['windows'][0]['placement'],'zoomed')
        self.assertTrue(report['windows'][0]['capabilities']['preferred_zoom'])
        window['lumen_state']['filled']=False
        with self.assertRaises(protocol.ProtocolError):state.snapshot(tree,outputs,'epoch')
        window['lumen_state']['filled']=True
        for key in ('zoomed','preferred_zoom'):
            window['lumen_state'][key]=1
            with self.assertRaises(protocol.ProtocolError):state.snapshot(tree,outputs,'epoch')
            window['lumen_state'][key]=True

    def test_unknown_is_not_inferred_from_scratchpad(self):
        tree,outputs,_=fixture();report=state.snapshot(tree,outputs,'epoch')
        w=report['windows'][0]
        self.assertIsNone(w['minimised']);self.assertIsNone(w['app_hidden']);self.assertIsNone(w['group_hidden'])
        self.assertFalse(report['capabilities']['application_registry'])
        self.assertNotIn('DO NOT EXPORT',str(report));self.assertNotIn('pid',w)
    def test_authoritative_extension_separates_minimised_and_filled(self):
        tree,outputs,w=fixture();w['lumen_state']={'minimised':True,'filled':True}
        report=state.snapshot(tree,outputs,'epoch');self.assertTrue(report['windows'][0]['minimised'])
        self.assertEqual(report['windows'][0]['placement'],'filled')
        self.assertIsNone(report['windows'][0]['app_hidden'])
    def test_duplicate_identity_and_ambiguous_focus_rejected(self):
        tree,outputs,w=fixture();tree['nodes'][0]['nodes'][0]['floating_nodes'].append(copy.deepcopy(w))
        with self.assertRaises(protocol.ProtocolError):state.snapshot(tree,outputs,'epoch')
        tree['nodes'][0]['nodes'][0]['floating_nodes'][1]['id']=8
        with self.assertRaises(protocol.ProtocolError):state.snapshot(tree,outputs,'epoch')
    def test_frame_ownership_is_not_invented_for_frameless_window(self):
        tree,outputs,w=fixture();w['border']='none'
        self.assertEqual(state.snapshot(tree,outputs,'epoch')['windows'][0]['frame_owner'],'unknown')
    def test_owned_socket_required_and_replacement_changes_epoch(self):
        with tempfile.TemporaryDirectory() as tmp:
            path=Path(tmp)/'endpoint'
            path.write_text('not a socket')
            with self.assertRaises(protocol.ProtocolError):state.runtime_epoch(path)
            path.unlink()
            with socket.socket(socket.AF_UNIX) as first:
                first.bind(str(path));one=state.runtime_epoch(path)
                link=Path(tmp)/'link';link.symlink_to(path)
                with self.assertRaises(protocol.ProtocolError):state.runtime_epoch(link)
                path.unlink()
                with socket.socket(socket.AF_UNIX) as second:
                    second.bind(str(path));self.assertNotEqual(one,state.runtime_epoch(path))
    def test_invalid_dimensions_rejected(self):
        tree,outputs,w=fixture();w['rect']['width']=-1
        with self.assertRaises(protocol.ProtocolError):state.snapshot(tree,outputs,'epoch')


if __name__=='__main__':unittest.main()

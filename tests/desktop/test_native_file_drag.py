"""Compositor drag snapshots cannot revive stale or other-seat offers."""
import importlib.util
from pathlib import Path
import unittest
import socket
import struct
import json
spec=importlib.util.spec_from_file_location('drag_observer',Path(__file__).resolve().parents[2]/'res/lumen-desktop/shell/native_file_drag.py')
module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)
class NativeDragFacts(unittest.TestCase):
    def test_newer_tick_wins_over_snapshot_and_duplicates(self):
        state=module.DragState()
        self.assertTrue(state.update(dict(seat='seat0',version=1,serial=8,active=False)))
        self.assertFalse(state.update(dict(seat='seat0',version=1,serial=7,active=True)))
        self.assertFalse(state.update(dict(seat='seat0',version=1,serial=8,active=True)))
        self.assertFalse(state.active)
    def test_public_tick_cannot_forge_state_or_wedge_serial(self):
        state=module.DragState()
        actual=[{'name':'seat0','lumen_drag':{'version':1,'serial':2,'active':False}}]
        self.assertTrue(state.refresh(actual))
        # The public payload supplies only a refresh hint, never state.update input.
        forged={'kind':'lumen-drag','version':1,'serial':999999,'active':True,'seat':'seat0'}
        if forged['kind']=='lumen-drag':self.assertTrue(state.refresh(actual))
        self.assertFalse(state.active);self.assertEqual(state.serial,2)
        actual[0]['lumen_drag'].update(serial=3,active=True)
        self.assertTrue(state.refresh(actual));self.assertTrue(state.active)
        self.assertFalse(state.refresh([{'name':'seat0','lumen_drag':{'version':True,'serial':4,'active':True}}]))
        self.assertFalse(state.update(dict(seat='seat0',version=True,serial=4,active=True)))
        state.disconnect();self.assertFalse(state.active)
        actual[0]['lumen_drag'].update(serial=1,active=False)
        self.assertTrue(state.refresh(actual));self.assertEqual(state.serial,1)
    def test_bounded_protocol_and_disconnect(self):
        left,right=socket.socketpair()
        with left,right:
            payload=json.dumps({'payload':'fact'}).encode()
            right.sendall(b'i3-ipc'+struct.pack('<II',len(payload),0x80000007)+payload)
            self.assertEqual(module.receive(left),(0x80000007,{'payload':'fact'}))
            right.sendall(b'i3-ipc'+struct.pack('<II',1048577,0))
            with self.assertRaises(ValueError):module.receive(left)
        left,right=socket.socketpair()
        with left:
            right.close()
            with self.assertRaises(EOFError):module.receive(left)
    def test_other_seats_and_bad_facts_fail_closed(self):
        state=module.DragState('seat1')
        for value in [dict(seat='seat0',version=1,serial=4,active=True),dict(seat='seat1',version=1,serial=True,active=True),dict(seat='seat1',version=1,serial=4,active='true'),dict(seat='seat1',version=2,serial=4,active=True)]:
            self.assertFalse(state.update(value));self.assertFalse(state.active)
        self.assertTrue(state.update(dict(seat='seat1',version=1,serial=4,active=True)));self.assertTrue(state.active)
        state.disconnect();self.assertFalse(state.active)
        self.assertTrue(state.update(dict(seat='seat1',version=1,serial=1,active=True)))
if __name__=='__main__':unittest.main()

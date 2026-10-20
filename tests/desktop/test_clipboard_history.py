import importlib.util
import io
import json
import os
from pathlib import Path
import tempfile
import unittest
from unittest import mock

spec=importlib.util.spec_from_file_location('clipboard_history',Path(__file__).resolve().parents[2]/'res/lumen-desktop/runtime/clipboard_history.py')
m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m)


class ClipboardPrivacy(unittest.TestCase):
    def setUp(self):
        self.tmp=tempfile.TemporaryDirectory();self.base=Path(self.tmp.name);self.clock=[1]
        self.history=m.History(self.base/'runtime',self.base/'state',limit=2,retention=60,clock=lambda:self.clock[0],wall=lambda:123)
    def tearDown(self):self.tmp.cleanup()
    def test_no_preconsent_or_sensitive_collection(self):
        self.assertFalse(self.history.ingest('before consent')['recorded'])
        self.history.set_enabled(True)
        self.assertFalse(self.history.ingest('marked secret',sensitive=True)['recorded'])
        self.assertEqual(self.history.list()['entries'],[])
    def test_disabled_legacy_ingest_never_reads_input_or_probes_selection(self):
        class UnreadableInput:
            @property
            def buffer(self):raise AssertionError('Disabled ingestion read input')
        output=io.StringIO()
        with mock.patch.object(m,'history',return_value=self.history), mock.patch.object(m.sys,'argv',['clipboard-history','ingest']), mock.patch.object(m.sys,'stdin',UnreadableInput()), mock.patch.object(m.sys,'stdout',output), mock.patch.object(m.subprocess,'run',side_effect=AssertionError('Disabled ingestion probed selection')):
            m.main()
        self.assertEqual(json.loads(output.getvalue()),{'recorded':False})
    def test_copy_requires_deliberate_action_and_original_text(self):
        self.history.set_enabled(True);text='Ü'*5000;self.history.ingest(text)
        entry=self.history.list()['entries'][0];self.assertTrue(entry['truncated']);copied=[]
        self.history.copy(entry['id'],copied.append);self.assertEqual(copied,[text])
    def test_clear_disable_and_expiry_prevent_stale_copy(self):
        self.history.set_enabled(True);self.history.ingest('one');item=self.history.list()['entries'][0]['id']
        self.clock[0]=62
        with self.assertRaises(ValueError):self.history.copy(item,lambda text:self.fail('stale copy'))
        self.history.ingest('two');self.history.clear();self.assertEqual(self.history.list()['entries'],[])
        self.history.ingest('three');self.history.set_enabled(False);self.assertEqual(self.history.list()['entries'],[])
    def test_capture_barrier_rejects_preclear_and_accepts_new_event(self):
        self.history.set_enabled(True);self.history.ingest('old',captured_at=1)
        self.clock[0]=2;self.history.clear()
        self.assertFalse(self.history.ingest('in flight',captured_at=1)['recorded'])
        self.clock[0]=3;self.assertTrue(self.history.ingest('new',captured_at=3)['recorded'])
        self.assertFalse(self.history.ingest('future',captured_at=4)['recorded'])
        self.assertEqual([e['text'] for e in self.history.list()['entries']],['new'])
        self.assertEqual(json.loads((self.history.runtime/'clear-state.json').read_text())['time'],2)
        self.assertEqual([p.name for p in self.history.state.iterdir()],['consent'])
    def test_revoked_interval_and_old_observer_epoch_cannot_replay(self):
        self.clock[0]=100;self.history.set_enabled(True);epoch=self.history.barrier()['epoch']
        self.clock[0]=101;self.history.set_enabled(False);self.clock[0]=102;self.history.set_enabled(True)
        self.assertFalse(self.history.ingest('old',captured_at=100)['recorded'])
        self.assertFalse(self.history.ingest('during revoke',captured_at=101.5)['recorded'])
        self.assertFalse(self.history.ingest('old observer',captured_at=102,epoch=epoch)['recorded'])
        self.assertTrue(self.history.ingest('new',captured_at=102,epoch=self.history.barrier()['epoch'])['recorded'])
    def test_delayed_ingest_uses_capture_age_and_cannot_refresh_retention(self):
        self.clock[0]=100;self.history.set_enabled(True);self.clock[0]=1000
        self.assertFalse(self.history.ingest('expired queue',captured_at=100)['recorded'])
        self.assertTrue(self.history.ingest('recent',captured_at=990)['recorded'])
        self.assertEqual(self.history.entries()[0]['created_monotonic'],990)
        self.clock[0]=1051;self.assertEqual(self.history.list()['entries'],[])
    def test_bounded_and_deduplicated(self):
        self.history.set_enabled(True)
        for text in ['one','one','two','three']:self.history.ingest(text)
        self.assertEqual([x['text'] for x in self.history.list()['entries']],['three','two'])
        self.assertEqual([x['text'] for x in self.history.list('THR')['entries']],['three'])
        self.assertFalse(self.history.ingest('x'*(m.MAX_BYTES+1))['recorded'])
    def test_files_private_and_history_not_persistent(self):
        self.history.set_enabled(True);self.history.ingest('synthetic text')
        self.assertEqual(self.history.data.stat().st_mode&0o777,0o600)
        self.assertEqual(self.history.runtime.stat().st_mode&0o777,0o700)
        self.assertEqual([p.name for p in self.history.state.iterdir()],['consent'])
    def test_symlink_history_target_never_read(self):
        self.history.set_enabled(True);target=self.base/'private';target.write_text('unrelated')
        self.history.data.symlink_to(target)
        with self.assertRaises(OSError):self.history.list()
        self.assertEqual(target.read_text(),'unrelated')
    def test_backwards_monotonic_data_is_discarded(self):
        self.history.set_enabled(True);self.history.ingest('one');self.clock[0]=0
        self.assertEqual(self.history.list()['entries'],[])


if __name__=='__main__':unittest.main()

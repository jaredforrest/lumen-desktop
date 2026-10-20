"""Durable placement is bounded private data, not a persisted window capability."""
import json
import os
from pathlib import Path
import sys
import tempfile
import subprocess
import time
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / 'res/lumen-desktop/runtime'))
import owned_geometry as g


class OwnedGeometry(unittest.TestCase):
    def placement(self, **values):
        return dict(output='HEADLESS-1', x=20, y=30, width=800, height=600, **values)

    def test_only_explicit_singleton_keys_and_lossless_exact_schema(self):
        for key in ('files.1', 'System Settings', '42', '', None, [], {}):
            with self.assertRaises(ValueError):g.owner_key(key)
        for key in g.OWNERS:self.assertEqual(g.owner_key(key), key)
        for field, value in [('width', True), ('height', 32769), ('x', 1.5), ('output', '\n')]:
            data=self.placement();data[field]=value
            with self.assertRaises(ValueError):g.record(data)
        data=self.placement();data['pid']=42
        with self.assertRaises(ValueError):g.record(data)

    def test_reopen_and_atomic_two_owner_preservation(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory=Path(temporary)/'state';store=g.Store(directory)
            self.assertIsNone(store.get('settings.main'))
            self.assertTrue(store.put('settings.main', self.placement()))
            self.assertFalse(store.put('settings.main', self.placement()))
            other=self.placement();other['output']='HEADLESS-2'
            g.Store(directory).put('calendar.main', other)
            reopened=g.Store(directory)
            self.assertEqual(reopened.get('settings.main'), self.placement())
            self.assertEqual(reopened.get('calendar.main'), other)
            self.assertEqual(reopened.path.stat().st_mode & 0o777, 0o600)
            self.assertEqual(directory.stat().st_mode & 0o777, 0o700)

    def test_malformed_state_refuses_without_overwriting(self):
        with tempfile.TemporaryDirectory() as temporary:
            store=g.Store(temporary);store.put('settings.main', self.placement())
            for value in ('{', json.dumps({'version': True,'owners': {}}), json.dumps({'version':1,'owners':{'foreign': self.placement()}})):
                store.path.write_text(value);before=store.path.read_bytes()
                with self.assertRaises(ValueError):store.put('calendar.main', self.placement())
                self.assertEqual(store.path.read_bytes(), before)

    def test_symlinks_permissions_and_oversize_refused(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary);store=g.Store(root/'state');target=root/'target';target.write_text('untouched')
            store.get('settings.main');store.path.symlink_to(target)
            with self.assertRaises(OSError):store.get('settings.main')
            self.assertEqual(target.read_text(), 'untouched');store.path.unlink()
            store.path.write_bytes(b' '* (g.MAX_BYTES+1));store.path.chmod(0o600)
            with self.assertRaises(ValueError):store.get('settings.main')
            store.path.write_text('{}');store.path.chmod(0o644)
            with self.assertRaises(ValueError):store.get('settings.main')

    def test_role_and_altered_layout_exclusions_and_reachable_restore(self):
        import copy
        row=({'type':'floating_con','rect':{'x':50,'y':60,'width':802,'height':601},
              'window_rect':{'width':800,'height':600},'deco_rect':{'height':32},
              'lumen_relationships':{'parent_supported':True,'parent':None,'modal':False}},
             {'name':'one','rect':{'x':960,'y':28,'width':800,'height':500}}, True)
        saved=self.placement();saved.update(x=600,y=500,width=1200,height=900)
        plan=g.restore_plan(row,saved,480,320,32,32)
        self.assertEqual((plan['width'],plan['height']),(766,435))
        self.assertGreaterEqual(plan['x'],960);self.assertGreaterEqual(plan['y'],28)
        self.assertLessEqual(plan['x']+plan['outerWidth'],1760)
        self.assertLessEqual(plan['y']+plan['outerHeight'],528)
        for patch in ({'lumen_state':{'zoomed':True}}, {'lumen_state':{'hidden':True}}, {'lumen_state':{'group_hidden':True}},
                      {'fullscreen_mode':1}, {'lumen_pair':{'survivor':True}},
                      {'lumen_relationships':{'parent_supported':True,'parent':42}},
                      {'lumen_relationships':{'parent_supported':True,'modal':True}},
                      {'lumen_relationships':{}}):
            changed=copy.deepcopy(row);changed[0].update(patch)
            self.assertIsNone(g.restore_plan(changed,saved,480,320))
            self.assertIsNone(g.observed(changed,'HEADLESS-1'))

    def test_published_opt_in_is_strict_and_failure_closed(self):
        with tempfile.TemporaryDirectory() as temporary:
            path=Path(temporary)/'appearance.json'
            with patch.dict(os.environ,{'LUMEN_APPEARANCE_STATE':str(path)}):
                self.assertFalse(g.enabled())
                for value in ({}, {'windowsPreferences':{'rememberOwnedGeometry':1}}, {'windowsPreferences':None}):
                    path.write_text(json.dumps(value));self.assertFalse(g.enabled())
                path.write_text(json.dumps({'windowsPreferences':{'rememberOwnedGeometry':True}}));self.assertTrue(g.enabled())
                path.write_text('{');self.assertFalse(g.enabled())

    def test_fifo_is_rejected_without_waiting_for_a_writer(self):
        with tempfile.TemporaryDirectory() as temporary:
            store=g.Store(temporary);os.mkfifo(store.path,0o600)
            started=time.monotonic()
            with self.assertRaisesRegex(ValueError,'unsafe'):store.get('settings.main')
            self.assertLess(time.monotonic()-started,.5)

    def test_second_process_lock_has_bounded_visible_refusal(self):
        with tempfile.TemporaryDirectory() as temporary:
            store=g.Store(temporary);store.put('settings.main',self.placement());before=store.path.read_bytes()
            script="import fcntl,sys,time;f=open(sys.argv[1],'a');fcntl.flock(f,fcntl.LOCK_EX);print('ready',flush=True);time.sleep(10)"
            holder=subprocess.Popen([sys.executable,'-c',script,str(Path(temporary)/'.placements.lock')],stdout=subprocess.PIPE,text=True)
            try:
                self.assertEqual(holder.stdout.readline().strip(),'ready')
                started=time.monotonic()
                with self.assertRaisesRegex(ValueError,'state is busy'):store.put('calendar.main',self.placement())
                self.assertLess(time.monotonic()-started,1.5)
                self.assertEqual(store.path.read_bytes(),before)
            finally:
                holder.terminate();holder.wait(timeout=3);holder.stdout.close()

    def test_failed_publication_keeps_previous_record_and_cleans_temp(self):
        with tempfile.TemporaryDirectory() as temporary:
            store=g.Store(temporary);store.put('settings.main', self.placement());before=store.path.read_bytes()
            with patch.object(g.os, 'replace', side_effect=OSError('synthetic publication failure')):
                with self.assertRaises(OSError):store.put('calendar.main', self.placement())
            self.assertEqual(store.path.read_bytes(), before)
            self.assertEqual(list(Path(temporary).glob('.owned-geometry-*')), [])


if __name__ == '__main__':unittest.main()

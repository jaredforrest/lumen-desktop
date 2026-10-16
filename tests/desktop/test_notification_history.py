"""Corrupt-state recovery tests; fixture files and a private D-Bus only."""
import copy
import importlib.util
import json
import os
from pathlib import Path
import stat
import subprocess
import sys
import tempfile
import time
import unittest

SOURCE = Path(__file__).resolve().parents[2] / 'res/lumen-desktop/notifications/service.py'
spec = importlib.util.spec_from_file_location('notification_history_service', SOURCE)
service = importlib.util.module_from_spec(spec)
spec.loader.exec_module(service)

ROW = dict(id=7, app='Fixture', desktop='fixture.desktop', summary='Saved notification',
           body='Retained history', icon='dialog-information', image='', urgency=1,
           resident=False, transient=False, timestamp=1791000000000, unread=True)


class HistoryTests(unittest.TestCase):
    def restore(self, state, raw=None):
        directory = tempfile.TemporaryDirectory(prefix='notification-history-')
        self.addCleanup(directory.cleanup)
        folder = Path(directory.name)
        history = folder / 'history.json'
        original = raw if raw is not None else json.dumps(state).encode()
        history.write_bytes(original)
        result = service.restore_history(history)
        backups = list(folder.glob('history-invalid-*.json'))
        for backup in backups:
            self.assertEqual(backup.read_bytes(), original)
            self.assertEqual(stat.S_IMODE(backup.stat().st_mode), 0o600)
        return result, backups

    def test_excessively_nested_json_is_quarantined(self):
        recovered, backups = self.restore(None, raw=b'{"rows":' + b'[' * 20000 + b'0' + b']' * 20000 + b'}')
        self.assertEqual(recovered, ([], False, 1))
        self.assertEqual(len(backups), 1)

    def test_valid_history_retained_and_actions_reset(self):
        row = dict(ROW, active=True, actions={'bad': ['shape']}, held=True, heldBy=['bad'], sender=':1.22')
        (rows, dnd, next_id), backups = self.restore({'rows': [row], 'dnd': True, 'nextId': 8})
        self.assertFalse(backups)
        self.assertEqual((dnd, next_id), (True, 8))
        self.assertEqual(rows[0]['summary'], ROW['summary'])
        self.assertEqual(rows[0]['actions'], [])
        self.assertFalse(rows[0]['active'])
        self.assertFalse(rows[0]['banner'])
        self.assertEqual(rows[0]['sender'], '')
        self.assertNotIn('heldBy', rows[0])
        self.assertNotIn('held', rows[0])

    def test_malformed_rows_do_not_discard_valid_neighbours(self):
        bad_values = [None, [], {}, {'id': 1}]
        for key, value in [('body', {}), ('app', []), ('id', True), ('id', 0),
                           ('id', 2**32), ('timestamp', float('inf')), ('urgency', 7),
                           ('unread', 'yes'), ('transient', True), ('image', {}),
                           ('body', '\ud800'), ('body', 'x' * 8193)]:
            bad_values.append(dict(ROW, **{key: value}))
        following = dict(ROW, id=8, summary='Following valid record')
        (rows, _, _), backups = self.restore({'rows': [ROW, *bad_values, following, ROW]})
        self.assertEqual([r['id'] for r in rows], [7, 8])
        self.assertEqual(len(backups), 1)

    def test_invalid_headers_preserve_valid_rows(self):
        (rows, dnd, next_id), backups = self.restore({'rows': [ROW], 'dnd': 'false', 'nextId': []})
        self.assertEqual([r['id'] for r in rows], [7])
        self.assertFalse(dnd)
        self.assertEqual(next_id, 1)
        self.assertEqual(len(backups), 1)

    def test_unreadable_formats_quarantined(self):
        for raw in [b'{"rows":', b'\xff', b'[]', b'null', b'{"rows":{}}']:
            with self.subTest(raw=raw):
                (rows, dnd, next_id), backups = self.restore(None, raw=raw)
                self.assertEqual((rows, dnd, next_id), ([], False, 1))
                self.assertEqual(len(backups), 1)

    def test_remote_images_never_restored(self):
        (rows, _, _), _ = self.restore({'rows': [dict(ROW, image='https://invalid.test/x.png', icon='https://invalid.test/icon')]})
        self.assertEqual(rows[0]['image'], '')
        self.assertEqual(rows[0]['icon'], '')

    def test_private_service_starts_and_restart_preserves_recovered_history(self):
        # The runner starts this test file with --private under dbus-run-session.
        result = subprocess.run(['dbus-run-session', '--', sys.executable, '-B', __file__, '--private'],
                                capture_output=True, text=True, timeout=20)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn('PASS private service recovery', result.stdout)


def private_service_test():
    import dbus
    with tempfile.TemporaryDirectory(prefix='notification-private-') as directory:
        folder = Path(directory)
        history = folder / 'history.json'
        attachment = folder / 'image-8.png'
        attachment.write_bytes(b'attachment fixture')
        history.write_text(json.dumps({'rows': [None, ROW, {}, dict(ROW, id=8, body={}, image=attachment.as_uri())], 'dnd': True, 'nextId': 8}))
        runtime=folder/'runtime';compiled=runtime/'lumen-nix-settings/current';compiled.mkdir(parents=True)
        (compiled/'settings.json').write_text(json.dumps({'notifications':{'policies':{}}}))
        bus = dbus.SessionBus()
        for iteration in range(2):
            with (folder / ('service-%s.log' % iteration)).open('w') as output:
                proc = subprocess.Popen([sys.executable, '-B', str(SOURCE), 'serve'],
                                        env={**os.environ, 'LUMEN_NOTIFICATION_STATE': directory, 'XDG_RUNTIME_DIR': str(runtime)}, stdout=output, stderr=output)
                try:
                    end = time.monotonic() + 5
                    while not bus.name_has_owner(service.BUS):
                        assert proc.poll() is None, 'Service exited during recovery'
                        assert time.monotonic() < end, 'Service failed to acquire name'
                        time.sleep(.025)
                    interface = dbus.Interface(bus.get_object(service.BUS, service.PATH), service.UI)
                    state = json.loads(interface.Snapshot())
                    assert state['ready'] and state['dnd']
                    assert [r['id'] for r in state['rows']] == [7]
                    assert not state['rows'][0]['active'] and not state['rows'][0]['actions']
                    # Exercise normal persistence through the actual D-Bus API.
                    interface.Command(json.dumps({'action': 'seen'}))
                    assert not json.loads(interface.Snapshot())['rows'][0]['unread']
                finally:
                    proc.terminate()
                    proc.wait(timeout=5)
                assert proc.returncode in (0, -15)
            deadline = time.monotonic() + 3
            while bus.name_has_owner(service.BUS):
                assert time.monotonic() < deadline
                time.sleep(.025)
        assert len(list(folder.glob('history-invalid-*.json'))) == 1
        saved_image = next(folder.glob('history-invalid-*.images/image-8.png'))
        assert saved_image.read_bytes() == b'attachment fixture'
        assert stat.S_IMODE(saved_image.stat().st_mode) == 0o600
        assert not attachment.exists()  # Normal collection leaves recovery copy intact.
        assert stat.S_IMODE(history.stat().st_mode) == 0o600
        assert len(json.loads(history.read_text())['rows']) == 1
    print('PASS private service recovery, API mutation and clean restart')


if __name__ == '__main__':
    if '--private' in sys.argv:
        private_service_test()
    else:
        unittest.main(verbosity=2)

"""Notification replacement/action regression under a disposable D-Bus session."""
import importlib.util
import json
from pathlib import Path
from notification_test_authority import UnlockedAuthority
import tempfile
import unittest
import dbus
import dbus.mainloop.glib

source = Path(__file__).resolve().parents[2]/'res/lumen-desktop/notifications/service.py'
spec = importlib.util.spec_from_file_location('notification_action_service', source)
service = importlib.util.module_from_spec(spec)
spec.loader.exec_module(service)
service.read_settings = lambda: {'notifications': {'policies': {}}}
dbus.mainloop.glib.DBusGMainLoop(set_as_default=True)


class ActionIdentityTests(unittest.TestCase):
    def test_replaced_delivery_rejects_stale_actions_and_focus_does_not_replay(self):
        with tempfile.TemporaryDirectory() as directory:
            server = service.Server(dbus.SessionBus(), Path(directory),lock_authority=UnlockedAuthority())
            actions = []
            server.ActionInvoked = lambda ident, key: actions.append((ident, key))
            sender = server.bus.get_unique_name()
            try:
                for malformed in ([], None, {'action': 'dnd', 'enabled': 'false'}, {'action': 'dismiss', 'id': True}, {'action': 'unknown'}):
                    with self.assertRaises(ValueError):
                        server.Command(json.dumps(malformed))
                self.assertFalse(server.dnd)
                ident = server.Notify('Fixture', 0, '', 'First', '', ['open', 'Open'], {}, -1, sender=sender)
                first = server.rows[0]['delivery']
                server.Notify('Fixture', ident, '', 'Replacement', '', ['open', 'Open other'], {}, -1, sender=sender)
                current = server.rows[0]['delivery']
                self.assertNotEqual(first, current)
                server.Command(json.dumps({'action': 'invoke', 'id': int(ident), 'key': 'open', 'delivery': first}))
                self.assertEqual(actions, [])
                self.assertTrue(server.rows[0]['active'])
                server.Command(json.dumps({'action': 'dnd', 'enabled': True}))
                server.Notify('Fixture', 0, '', 'Quiet', '', [], {}, -1, sender=sender)
                self.assertFalse(any(row['banner'] for row in server.rows))
                server.Command(json.dumps({'action': 'dnd', 'enabled': False}))
                self.assertFalse(any(row['banner'] for row in server.rows))
                server.Command(json.dumps({'action': 'invoke', 'id': int(ident), 'key': 'open', 'delivery': current}))
                self.assertEqual(actions, [(ident, 'open')])
                server.Command(json.dumps({'action': 'invoke', 'id': int(ident), 'key': 'open', 'delivery': current}))
                self.assertEqual(len(actions), 1)
                self.assertNotIn('delivery', (Path(directory)/'history.json').read_text())
            finally:
                service.GLib.source_remove(server.tick_source)
                server.bus.remove_signal_receiver(server.owner_changed, 'NameOwnerChanged', 'org.freedesktop.DBus')
                server.remove_from_connection()


if __name__ == '__main__':
    unittest.main()

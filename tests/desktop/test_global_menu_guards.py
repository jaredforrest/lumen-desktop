"""Menu authority fixtures without registering a desktop bus name or running IPC."""
import importlib.util
from pathlib import Path
import unittest
from types import SimpleNamespace

source = Path(__file__).resolve().parents[2]/'res/lumen-desktop/global-menu/bridge.py'
spec = importlib.util.spec_from_file_location('menu_guard_fixture', source)
bridge = importlib.util.module_from_spec(spec)
spec.loader.exec_module(bridge)


def layout(enabled=True, visible=True):
    return (0, {}, [(1, {'enabled': enabled, 'visible': visible, 'children-display': 'submenu'}, [(2, {'label': 'Run'}, [])])])


class MenuGuardTests(unittest.TestCase):
    def test_disabled_and_hidden_ancestors_reject_action(self):
        for enabled, visible in [(False, True), (True, False)]:
            tree = bridge.clean(layout(enabled, visible))
            self.assertIsNotNone(bridge.lookup(tree, 2))
            self.assertIsNone(bridge.actionable(tree, 2))
        self.assertEqual(bridge.actionable(bridge.clean(layout()), 2)['label'], 'Run')

    def test_separators_and_missing_nodes_are_not_actions(self):
        tree = bridge.clean((0, {}, [(2, {'type': 'separator'}, [])]))
        self.assertIsNone(bridge.actionable(tree, 2))
        self.assertIsNone(bridge.actionable(tree, 99))

    def test_activate_api_never_dispatches_disabled_ancestor(self):
        events = []
        fake = SimpleNamespace(guarded=lambda token: object(), refresh=lambda: None,
                               proxy=lambda item: SimpleNamespace(Event=lambda *args, **kwargs: events.append(args)),
                               state={'menu': bridge.clean(layout(False, True))}, last_refresh=0)
        self.assertFalse(bridge.Bridge.Activate(fake, 'captured', 2))
        self.assertEqual(events, [])
        fake.state['menu'] = bridge.clean(layout())
        self.assertTrue(bridge.Bridge.Activate(fake, 'captured', 2))
        self.assertEqual(len(events), 1)

    def test_invalid_refresh_removes_old_menu(self):
        fake = SimpleNamespace(current=lambda: ({'id': 1, 'name': 'Fixture', 'app_id': 'fixture'}, ('sender', '/', 1), 'token'),
                               held_token='', window_context={}, output_name='fixture', proxy=lambda item: SimpleNamespace(GetLayout=lambda *args, **kwargs: (0, (0, [], []))),
                               publish=lambda: None, state={'menu': bridge.clean(layout()), 'supported': True})
        bridge.Bridge.refresh(fake)
        self.assertFalse(fake.state['supported'])
        self.assertIsNone(fake.state['menu'])
        self.assertEqual(fake.state['error'], 'Invalid menu layout')

    def test_malformed_identity_and_availability_are_rejected(self):
        for tree in [(0, {}, [(1, {}, []), (1, {}, [])]), (0, {'enabled': 'false'}, []), (True, {}, []), (0, [], [])]:
            with self.assertRaises(ValueError):
                bridge.clean(tree)

    def test_exported_layout_is_bounded(self):
        tree = (1, {}, [])
        for index in range(34):
            tree = (index+2, {}, [tree])
        with self.assertRaises(ValueError):
            bridge.clean(tree)
        with self.assertRaises(ValueError):
            bridge.clean((0, {}, [(n+1, {}, []) for n in range(4096)]))
        tree = bridge.clean((0, {}, [(1, {'label': 'x'*10000}, [])]))
        self.assertEqual(len(tree['children'][0]['label']), 4096)


if __name__ == '__main__':
    unittest.main()

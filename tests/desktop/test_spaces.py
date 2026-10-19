import importlib.util
from pathlib import Path
import tempfile
import unittest

spec = importlib.util.spec_from_file_location('spaces', Path(__file__).parents[2] / 'res/lumen-desktop/runtime/spaces.py')
spaces = importlib.util.module_from_spec(spec)
spec.loader.exec_module(spaces)


class PolicyTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.path = Path(self.tmp.name) / 'spaces.json'
        self.policy = spaces.SpacePolicy(self.path)
        self.outputs = [dict(name='A', make='Acme', model='Panel', serial='123', active=True)]
        self.workspaces = [dict(id=i, name=str(i), output='A', visible=i == 1) for i in range(1, 4)]
        self.policy.reconcile(self.workspaces, self.outputs)

    def test_identity_survives_restart_and_session_rename(self):
        key = self.policy.current[1]
        self.workspaces[0]['name'] = 'Work'
        self.policy.reconcile(self.workspaces, self.outputs)
        self.assertEqual(key, self.policy.current[1])
        other = spaces.SpacePolicy(self.path)
        self.workspaces[0]['id'] = 90
        other.reconcile(self.workspaces, self.outputs)
        self.assertEqual(key, other.current[90])

    def test_order_never_renames_or_reidentifies(self):
        original = dict(self.policy.current)
        self.policy.focus(2)
        self.policy.focus(3)
        self.assertEqual(self.policy.order(), [original[3], original[2], original[1]])
        self.policy.configure(auto_reorder=False)
        self.assertEqual(self.policy.order(), list(original.values()))
        self.assertEqual(self.policy.current, original)

    def test_exact_typed_assignment_and_persistence(self):
        self.policy.assign('wayland:editor', 'desktop', self.policy.current[2])
        self.assertEqual(self.policy.destination(dict(app_id='editor'))['workspace'], '2')
        self.assertEqual(self.policy.destination(dict(window_properties={'class': 'editor'}))['status'], 'unassigned')
        other = spaces.SpacePolicy(self.path)
        other.reconcile(self.workspaces, self.outputs)
        self.assertEqual(other.destination(dict(app_id='editor'))['workspace'], '2')

    def test_removed_display_retains_policy_without_moving(self):
        target = spaces.SpacePolicy.display(self.outputs[0])
        self.policy.assign('wayland:editor', 'display', target)
        self.assertEqual(self.policy.destination(dict(app_id='editor'))['workspace'], '1')
        self.policy.reconcile([], [])
        self.assertEqual(self.policy.destination(dict(app_id='editor'))['fallback'], 'keep-current-placement')
        self.policy.reconcile(self.workspaces, self.outputs)
        self.assertEqual(self.policy.destination(dict(app_id='editor'))['workspace'], '1')

    def test_transaction_reloads_other_process_policy(self):
        other = spaces.SpacePolicy(self.path)
        with other.transaction():
            other.assign('wayland:editor', 'desktop', self.policy.current[2])
        with self.policy.transaction():
            self.assertEqual(self.policy.destination(dict(app_id='editor'))['workspace'], '2')

    def test_adjacent_order_is_local_to_output(self):
        self.workspaces[2]['output'] = 'B'
        self.policy.reconcile(self.workspaces, self.outputs)
        self.policy.focus(3)
        self.assertEqual(self.policy.order('A'), [self.policy.current[1], self.policy.current[2]])

    def test_placement_never_requests_focus(self):
        from unittest.mock import patch
        self.policy.assign('wayland:editor', 'desktop', self.policy.current[2])
        with patch.object(spaces, 'sway', return_value=[{'success': True}]) as command:
            spaces.place(self.policy, dict(id=42, app_id='editor'))
            command.assert_called_once_with('[con_id=42] move container to workspace "2"')

    def test_unavailable_desktop_and_native_capability(self):
        self.policy.assign('wayland:editor', 'desktop', 'missing')
        self.assertEqual(self.policy.destination(dict(app_id='editor'))['status'], 'unavailable')
        with self.assertRaisesRegex(ValueError, 'all-window membership'):
            self.policy.assign('wayland:editor', 'all')


if __name__ == '__main__':
    unittest.main()

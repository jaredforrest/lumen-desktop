import importlib.util
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
spec = importlib.util.spec_from_file_location('corners', Path(__file__).parents[2] / 'res/lumen-desktop/runtime/hot_corners.py')
corners = importlib.util.module_from_spec(spec)
spec.loader.exec_module(corners)


class CornerTests(unittest.TestCase):
    def event(self, corner='top-left', **kwargs):
        return dict(version=1, kind='lumen-corner', corner=corner, output='A', modifiers=kwargs.get('modifiers', []), locked=kwargs.get('locked', False), fullscreen=kwargs.get('fullscreen', False), adjacent=kwargs.get('adjacent', False))

    def test_disabled_default(self):
        self.assertIsNone(corners.CornerPolicy().transition(self.event()))

    def test_all_corners_modifiers_latch_and_reentry(self):
        for corner in corners.CORNERS:
            policy = corners.CornerPolicy({corner: dict(action='overview', modifiers=['super', 'shift'])})
            self.assertIsNone(policy.transition(self.event(corner)))
            self.assertIsNone(policy.transition(self.event(corner, modifiers=['super', 'shift'])))
            policy.transition(self.event(None))
            self.assertEqual(policy.transition(self.event(corner, modifiers=['super', 'shift'])), 'overview')
            self.assertIsNone(policy.transition(self.event(corner, modifiers=['super', 'shift'])))

    def test_safeguards_and_explicit_opt_in(self):
        for safeguard in ('fullscreen', 'adjacent', 'locked'):
            policy = corners.CornerPolicy({'top-left': dict(action='lock')})
            self.assertIsNone(policy.transition(self.event(**{safeguard: True})))
        policy = corners.CornerPolicy({'top-left': dict(action='lock', allowFullscreen=True, allowAdjacent=True)})
        self.assertEqual(policy.transition(self.event(fullscreen=True, adjacent=True)), 'lock')
        policy.transition(self.event(None))
        self.assertIsNone(policy.transition(self.event(locked=True)))

    def test_validation_and_persistence(self):
        for value in ({'bad': {}}, {'top-left': {'action': 'exec rm'}}, {'top-left': {'modifiers': ['super', 'super']}}, {'top-left': {'allowFullscreen': 1}}):
            with self.assertRaises(ValueError):
                corners.validate(value)
        with tempfile.TemporaryDirectory() as private:
            path = Path(private) / 'config.json'
            corners.write_config(path, {'bottom-right': {'action': 'show-desktop'}})
            import json
            self.assertEqual(corners.validate(json.loads(path.read_text()))['bottom-right']['action'], 'show-desktop')

    def test_every_supported_action_has_exact_provider(self):
        for action in corners.ACTIONS - {'disabled'}:
            command = ['lumen-lock'] if action == 'lock' else ['lumen-app-launcher'] if action == 'app-launcher' else ['swaymsg', 'output * power off'] if action == 'display-sleep' else ['lumen-desktop-action', action]
            with patch.object(corners.subprocess, 'run') as run:
                corners.dispatch(action)
                run.assert_called_once_with(command, check=True, timeout=15)

    def test_missing_native_inhibitions_fail_closed(self):
        policy = corners.CornerPolicy({'top-left': dict(action='overview')})
        event = self.event()
        del event['locked']
        self.assertIsNone(policy.transition(event))
        self.assertIsNone(policy.transition(self.event()))
        policy.transition(self.event(None))
        self.assertEqual(policy.transition(self.event()), 'overview')

    def test_exact_dispatch_no_shell_interpolation(self):
        with patch.object(corners.subprocess, 'run') as run:
            corners.dispatch('overview')
            run.assert_called_once_with(['lumen-desktop-action', 'overview'], check=True, timeout=15)
        with self.assertRaises(ValueError):
            corners.dispatch('exec anything')


if __name__ == '__main__':
    unittest.main()

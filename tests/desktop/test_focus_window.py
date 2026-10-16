"""Focus fallback races, ambiguous document identity and scratchpad recovery.

Uses a fake Sway endpoint; never focuses or closes a real session window.
"""
import importlib.util
import json
import subprocess
import unittest
from pathlib import Path
from unittest.mock import patch

HELPER = Path(__file__).resolve().parents[2] / 'res/lumen-desktop/focus-window.py'
spec = importlib.util.spec_from_file_location('focus_window', HELPER)
focus = importlib.util.module_from_spec(spec)
spec.loader.exec_module(focus)


def window(identifier, *, focused=False, xwayland=False, title='Document'):
    return dict(id=identifier, type='con', pid=identifier + 100,
                app_id=None if xwayland else 'writer',
                window_properties={'class': 'writer'} if xwayland else None,
                name=title, focused=focused)


def tree(*nodes, workspace='1', floating=False):
    return dict(type='root', nodes=[dict(type='workspace', name=workspace,
                nodes=[] if floating else list(nodes),
                floating_nodes=list(nodes) if floating else [])])


class FocusWindowTests(unittest.TestCase):
    def test_unique_native_window_uses_container_identity(self):
        self.assertEqual(focus.focus_command(tree(window(17)), 'writer', 'Document'),
                         '[con_id=17] focus')

    def test_xwayland_floating_window(self):
        self.assertEqual(focus.focus_command(tree(window(18, xwayland=True), floating=True),
                                             'writer', 'Document'), '[con_id=18] focus')

    def test_activation_completed_before_delayed_fallback_is_noop(self):
        self.assertIsNone(focus.focus_command(tree(window(17, focused=True), window(18)),
                                             'writer', 'Document'))

    def test_ambiguous_documents_are_not_guessed(self):
        with self.assertRaisesRegex(ValueError, 'ambiguous'):
            focus.focus_command(tree(window(17), window(18)), 'writer', 'Document')

    def test_disappeared_or_renamed_window_is_not_replaced(self):
        with self.assertRaisesRegex(ValueError, 'no longer available'):
            focus.focus_command(tree(window(17, title='Different')), 'writer', 'Document')

    def test_minimized_window_restores_via_compositor_placement(self):
        self.assertEqual(focus.focus_command(tree(window(17), workspace='__i3_scratch'),
                                             'writer', 'Document'),
                         '[con_id=17] scratchpad show, focus')

    def test_actions_use_identity_even_when_the_window_is_focused(self):
        for action in ('close','minimize','fill','fullscreen'):
            self.assertEqual(focus.focus_command(tree(window(17,focused=True)), 'writer','Document',action),
                             '[con_id=17] lumen_window '+action)
        with self.assertRaisesRegex(ValueError,'Unsupported'):
            focus.focus_command(tree(window(17)), 'writer','Document','kill; exec bad')

    def test_hostile_title_never_enters_sway_command(self):
        title = '\"; kill; [app_id=.*]'
        self.assertEqual(focus.focus_command(tree(window(17, title=title)), 'writer', title),
                         '[con_id=17] focus')

    def test_main_reports_rejected_ipc_even_with_successful_process(self):
        with patch.object(focus.subprocess, 'check_output', side_effect=[
                json.dumps(tree(window(17))), '[{"success":false,"error":"gone"}]']):
            with self.assertRaisesRegex(ValueError, 'rejected'):
                focus.main(['writer', 'Document'])

    def test_main_success_and_bounded_ipc(self):
        with patch.object(focus.subprocess, 'check_output', side_effect=[
                json.dumps(tree(window(17))), '[{"success":true}]']) as run:
            self.assertEqual(focus.main(['writer', 'Document']), 0)
            self.assertEqual(run.call_args_list[1].args[0],
                             ['swaymsg', '-r', '[con_id=17] focus'])
            self.assertTrue(all(call.kwargs['timeout'] == 4 for call in run.call_args_list))

    def test_main_noop_never_sends_a_command(self):
        with patch.object(focus.subprocess, 'check_output', return_value=json.dumps(
                tree(window(17, focused=True), window(18)))) as run:
            self.assertEqual(focus.main(['writer', 'Document']), 0)
            self.assertEqual(run.call_count, 1)

    def test_ipc_timeout_is_not_retried_against_another_window(self):
        with patch.object(focus.subprocess, 'check_output', side_effect=
                          subprocess.TimeoutExpired('swaymsg', 4)) as run:
            with self.assertRaises(subprocess.TimeoutExpired):
                focus.main(['writer', 'Document'])
            self.assertEqual(run.call_count, 1)


if __name__ == '__main__':
    unittest.main()

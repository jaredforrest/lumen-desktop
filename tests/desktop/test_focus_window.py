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
    def test_modal_native_child_redirects_parent_activation_only(self):
        document=window(17)
        utility=window(18,title='Find');utility['lumen_relationships']=dict(version=1,parent_supported=True,parent=17,modal=None)
        self.assertEqual(focus.focus_command(tree(document,utility),'writer','Document'),'[con_id=17] focus')
        utility['lumen_relationships']['modal']=True
        self.assertEqual(focus.focus_command(tree(document,utility),'writer','Document'),'[con_id=18] focus')
        self.assertEqual(focus.app_focus_command(tree(document,utility),'writer',['Document']),'[con_id=18] focus')
        self.assertEqual(focus.focus_command(tree(document,utility),'writer','Document','close'),'[con_id=17] lumen_window close')


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

    def test_app_activation_uses_mru_not_tree_order(self):
        self.assertEqual(focus.app_focus_command(tree(window(17,title='First'),window(18,title='Last')),
                                                'writer',['Last','First']), '[con_id=18] focus')

    def test_no_space_switch_activates_current_space_candidate(self):
        root=dict(type='root',nodes=[tree(window(17,focused=True,title='Current'),workspace='1')['nodes'][0],
                                    tree(window(18,title='Other'),workspace='2')['nodes'][0]])
        self.assertIsNone(focus.app_focus_command(root,'writer',['Other','Current'],False))
        self.assertEqual(focus.app_focus_command(root,'writer',['Other','Current'],True),'[con_id=18] focus')
        self.assertIsNone(focus.app_focus_command(root,'writer',['Other'],False))

    def test_app_activation_never_guesses_duplicate_titles(self):
        with self.assertRaisesRegex(ValueError,'ambiguous'):
            focus.app_focus_command(tree(window(17),window(18)),'writer',['Document'])

    def test_app_activation_validates_title_list(self):
        with self.assertRaisesRegex(ValueError,'list of strings'):
            focus.app_focus_command(tree(window(17)),'writer','Document')

    def test_hidden_app_unhides_all_but_restores_only_selected_minimised_window(self):
        a=window(17,title='First');a['lumen_state']=dict(version=1,hidden=True,minimised=False,restore_workspace='1')
        b=window(18,title='Second');b['lumen_state']=dict(version=1,hidden=True,minimised=True,restore_workspace='1')
        result=focus.app_focus_command(tree(a,b,workspace='__i3_scratch'),'writer',['First','Second'])
        self.assertEqual(result,'[con_id=17] lumen_window unhide; [con_id=18] lumen_window unhide; [con_id=17] focus')
        result=focus.app_focus_command(tree(a,b,workspace='__i3_scratch'),'writer',['Second','First'])
        self.assertTrue(result.endswith('[con_id=18] scratchpad show, focus'))

    def test_hidden_window_original_workspace_respects_no_switch(self):
        hidden=window(18,title='Hidden');hidden['lumen_state']=dict(version=1,hidden=True,minimised=False,restore_workspace='1')
        root=dict(type='root',nodes=[tree(window(17,focused=True,title='Current'),workspace='1')['nodes'][0],
                                    tree(hidden,workspace='__i3_scratch')['nodes'][0]])
        self.assertEqual(focus.app_focus_command(root,'writer',['Hidden'],False),
                         '[con_id=18] lumen_window unhide; [con_id=18] focus')

    def test_hide_others_retains_target_application_and_uses_only_ids(self):
        a=window(17,title='First');b=window(18,title='Second')
        for node in (a,b):node['lumen_state']=dict(version=1)
        b['app_id']='other'
        self.assertEqual(focus.app_visibility_command(tree(a,b),'writer',['First'],'hide-others'),
                         '[con_id=18] lumen_window hide')
        self.assertEqual(focus.app_visibility_command(tree(a,b),'writer',['First'],'hide'),
                         '[con_id=17] lumen_window hide')

    def test_hiding_never_falls_back_to_minimisation_on_unpatched_backend(self):
        with self.assertRaisesRegex(ValueError,'independent application hiding'):
            focus.app_visibility_command(tree(window(17)),'writer',['Document'],'hide')

    def test_hide_current_app_activates_previous_application(self):
        a=window(17,title='Current');a['lumen_state']=dict(version=1)
        b=window(18,title='Previous');b['app_id']='other'
        with patch.object(focus.subprocess,'check_output',side_effect=[
                json.dumps(tree(a,b)), '[{"success":true},{"success":true}]']) as run:
            self.assertEqual(focus.main(['writer','','--activate-titles','["Current"]','--app-action','hide',
                                        '--previous-app','other','--previous-title','Previous']),0)
            self.assertEqual(run.call_args_list[1].args[0][-1],
                             '[con_id=17] lumen_window hide; [con_id=18] focus')

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

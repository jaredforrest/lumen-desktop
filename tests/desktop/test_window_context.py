"""Separate native parenthood, key focus and explicit document authority."""
import sys
from pathlib import Path
import unittest
sys.path.insert(0, str(Path(__file__).resolve().parents[2]/'res/lumen-desktop/runtime'))
from protocol import ProtocolError
from window_context import relationships, contexts, activation_target


def window(ident, app='editor', parent=None, modal=None):
    return dict(id=ident, app_hint=app, parent=parent, modal=modal)


class WindowContextTests(unittest.TestCase):
    def test_main_does_not_follow_key_panel_or_other_app(self):
        observed = [window(1), window(2, parent=1), window(3, app='other')]
        for key in (2, 1, 3):
            result = contexts(observed, {'editor':1})
            self.assertEqual(result[2]['main_window'], 1)
            self.assertEqual(result[key]['application_hint'], 'other' if key == 3 else 'editor')
            self.assertIsNone(result[3]['main_window'])
        self.assertIsNone(contexts(observed)[2]['main_window'])

    def test_parent_context_does_not_replace_keyboard_recipient(self):
        result = contexts([window(1), window(2, app='dialog-hint', parent=1)])
        self.assertEqual(result[2]['context_window'], 1)
        self.assertEqual(result[2]['application_hint'], 'editor')
        self.assertEqual(activation_target([window(1),window(2,parent=1)], 2), 2)
        self.assertEqual(activation_target([window(1),window(2,parent=1)], 1), 1)

    def test_explicit_modal_blocks_only_parent_branch(self):
        observed=[window(1),window(2,parent=1,modal=True),window(3,parent=2,modal=True),window(4)]
        self.assertEqual(activation_target(observed,1),3)
        self.assertEqual(activation_target(observed,4),4)
        observed.pop(2)
        self.assertEqual(activation_target(observed,1),2)
        observed[1]['parent']=4
        self.assertEqual(activation_target(observed,1),1)
        self.assertEqual(activation_target(observed,4),2)

    def test_ambiguous_modal_siblings_and_cycles_rejected(self):
        with self.assertRaises(ProtocolError):activation_target([window(1),window(2,parent=1,modal=True),window(3,parent=1,modal=True)],1)
        with self.assertRaises(ProtocolError):contexts([window(1,parent=2),window(2,parent=1)])
        with self.assertRaises(ProtocolError):activation_target([window(1,parent=2,modal=True),window(2,parent=1,modal=True)],1)

    def test_destroyed_parent_and_stale_adapter_are_not_recovered(self):
        result=contexts([window(2,parent=1)])
        self.assertIsNone(result[2]['parent'])
        self.assertEqual(result[2]['context_window'],2)
        with self.assertRaises(ProtocolError):contexts([window(2,parent=1)],{'editor':1})
        with self.assertRaises(ProtocolError):contexts([window(1),window(2,app='other')],{'editor':2})

    def test_typed_relationships_keep_unsupported_unknown(self):
        self.assertIsNone(relationships(None))
        valid=dict(version=1,parent_supported=True,parent=None,modal=None)
        self.assertEqual(relationships(valid)['modal'],None)
        for field,value in [('version',True),('parent',True),('modal',1),('parent_supported',1)]:
            with self.subTest(field=field),self.assertRaises(ProtocolError):relationships(dict(valid,**{field:value}))
        with self.assertRaises(ProtocolError):relationships(dict(valid,parent_supported=False,parent=2))


if __name__=='__main__':unittest.main()

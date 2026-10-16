"""Integration checks against the real Nix evaluator and isolated settings files."""
import importlib.util
import json
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location('nix_settings', ROOT/'res/lumen-desktop/shell/nix_settings.py')
settings = importlib.util.module_from_spec(spec)
spec.loader.exec_module(settings)


class NixSettingsTest(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.root = Path(self.directory.name)
        self.source = self.root/'desktop.nix'
        self.source.write_text('{}\n')
        self.environment = dict(os.environ)
        os.environ['XDG_RUNTIME_DIR'] = str(self.root/'runtime')
        os.environ['LUMEN_NIX_SETTINGS_SOURCE'] = str(self.source)

    def tearDown(self):
        os.environ.clear(); os.environ.update(self.environment)
        self.directory.cleanup()

    def test_invalid_value_preserves_source_and_active_bundle(self):
        settings.transaction()
        original = self.source.read_bytes()
        active = (settings.runtime()/'current').resolve()
        with self.assertRaises(ValueError):
            settings.transaction('appearance', {'mode': 'unsupported'})
        self.assertEqual(self.source.read_bytes(), original)
        self.assertEqual((settings.runtime()/'current').resolve(), active)

    def test_success_commits_nix_and_survives_runtime_loss(self):
        settings.transaction('appearance', {'mode': 'dark'})
        self.assertIn('"appearance" =', self.source.read_text())
        (settings.runtime()/'current').unlink()
        values = settings.transaction()
        self.assertEqual(values['appearance']['mode'], 'dark')
        self.assertTrue(str((settings.runtime()/'current').resolve()).startswith('/nix/store/'))

    def test_text_is_literal_not_nix_interpolation(self):
        text = '${builtins.abort "must not evaluate"}\nQuotes: " and \\ and é'
        values = settings.transaction('menuShortcuts', {'1': {'app': text, 'path': 'File/Save'}})
        self.assertEqual(values['menuShortcuts']['1']['app'], text)
        self.assertEqual(settings.transaction()['menuShortcuts']['1']['app'], text)

    def test_application_update_keeps_other_application_config(self):
        values=settings.transaction('applicationPreferences',{'ui.json':{'theme':'dark'}},application='wordcraft')
        self.assertEqual(values['applicationPreferences']['wordcraft']['ui.json']['theme'],'dark')
        self.assertEqual(values['applicationPreferences']['gridcraft']['ui.json']['dark'],False)
        with self.assertRaises(ValueError):
            settings.transaction('applicationPreferences',{'ui.json':{'recent':['private.docx']}},application='wordcraft')

    def test_live_controls_are_not_configuration_sections(self):
        for section in ('audio','brightness','files','dictionaryWords','snippets'):
            with self.assertRaises(ValueError):settings.transaction(section,{})
        with self.assertRaises(ValueError):
            settings.transaction('notifications',{'dnd':True,'policies':{}})

    def test_failed_system_activation_preserves_source(self):
        original=self.source.read_bytes()
        os.environ['LUMEN_NIX_SETTINGS_BUILD_SOURCE']='/unused-test-input'
        with patch.object(settings,'rebuild_system',return_value=Path('/nix/store/test-system')) as rebuild:
            with patch.object(settings.subprocess,'run',wraps=settings.subprocess.run) as run:
                actual=run._mock_wraps
                def execute(command,**kwargs):
                    if command[0]=='pkexec':raise ValueError('Authentication cancelled')
                    return actual(command,**kwargs)
                run.side_effect=execute
                with self.assertRaises(ValueError):settings.transaction('appearance',{'mode':'dark'})
            rebuild.assert_called_once()
        self.assertEqual(self.source.read_bytes(),original)

    def test_display_identity_and_unknown_sections(self):
        display = dict(name='DP-1', identity=['DP-1', 'Maker', 'Model', None],
                       mode='1920x1080@60.000Hz', scale=1.25, x=0, y=0)
        self.assertEqual(settings.transaction('displays', {'DP-1': display})['displays']['DP-1'], display)
        with self.assertRaises(ValueError): settings.transaction('unknown', {})


if __name__ == '__main__': unittest.main()

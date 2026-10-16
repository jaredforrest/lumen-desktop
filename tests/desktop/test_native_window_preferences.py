import json
import os
import socket
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

SCRIPT = Path(__file__).resolve().parents[2] / 'res/lumen-desktop/native_window_preferences.py'

class WindowPreferences(unittest.TestCase):
    def run_adapter(self, config, app, *args):
        subprocess.run([sys.executable, str(SCRIPT), app, *args], check=True,
                       env=dict(os.environ, XDG_CONFIG_HOME=str(config)))

    def test_browser_preserves_unrelated_state_and_updates_all_profiles(self):
        with tempfile.TemporaryDirectory() as temporary:
            config = Path(temporary)
            for name in ['Default', 'Profile 1']:
                file = config / 'BraveSoftware/Brave-Browser' / name / 'Preferences'
                file.parent.mkdir(parents=True)
                original = {'browser': {'custom_chrome_frame': True, 'other': 7}, 'session': {'fixture': 'retain'}}
                file.write_text(json.dumps(original))
            self.run_adapter(config, 'brave')
            for file in config.glob('BraveSoftware/Brave-Browser/*/Preferences'):
                data = json.loads(file.read_text())
                self.assertEqual(data['browser'], {'custom_chrome_frame': False, 'other': 7})
                self.assertEqual(data['session'], {'fixture': 'retain'})
                self.assertEqual(data['extensions']['theme']['system_theme'], 1)
                self.assertFalse(data['brave']['wallet']['show_wallet_icon_on_toolbar'])
            local = config / 'BraveSoftware/Brave-Browser/Local State'
            self.assertTrue(json.loads(local.read_text())['brave']['tabs']['compact_horizontal_tabs'])

    def test_gimp_system_theme_preserves_editing_preferences(self):
        with tempfile.TemporaryDirectory() as temporary:
            config=Path(temporary)
            file=config/'GIMP/3.2/gimprc';file.parent.mkdir(parents=True)
            file.write_text('# retained comment\n(theme-color-scheme dark)\n(undo-levels 24)\n')
            self.run_adapter(config,'gimp')
            self.assertEqual(file.read_text(),'# retained comment\n(theme-color-scheme system)\n(undo-levels 24)\n')
            before=file.read_bytes();self.run_adapter(config,'gimp');self.assertEqual(file.read_bytes(),before)

    def test_running_app_profile_is_not_modified(self):
        with tempfile.TemporaryDirectory() as temporary:
            config = Path(temporary)
            root = config / 'obsidian'; root.mkdir()
            file = root / 'obsidian.json'; file.write_text('{"frame":"hidden","vaults":{"fixture":{}}}')
            (root / 'SingletonLock').symlink_to('fixture-12345')
            before = file.read_bytes()
            self.run_adapter(config, 'obsidian')
            self.assertEqual(file.read_bytes(), before)
            (root / 'SingletonLock').unlink()
            self.run_adapter(config, 'obsidian')
            self.assertEqual(json.loads(file.read_text()), {'frame': 'native', 'vaults': {'fixture': {}}})

    def test_explicit_browser_profile_is_respected(self):
        with tempfile.TemporaryDirectory() as temporary:
            config = Path(temporary)
            custom = config / 'custom-profile'
            self.run_adapter(config, 'brave', '--user-data-dir', str(custom))
            self.assertFalse((config / 'BraveSoftware').exists())
            self.assertFalse(json.loads((custom / 'Default/Preferences').read_text())['browser']['custom_chrome_frame'])

    def test_stale_lock_after_reboot_does_not_block_frame_preference(self):
        with tempfile.TemporaryDirectory() as temporary:
            config = Path(temporary)
            root = config / 'obsidian'; root.mkdir()
            (root / 'SingletonLock').symlink_to(socket.gethostname() + '-2147483646')
            self.run_adapter(config, 'obsidian')
            self.assertEqual(json.loads((root / 'obsidian.json').read_text())['frame'], 'native')

if __name__ == '__main__': unittest.main()

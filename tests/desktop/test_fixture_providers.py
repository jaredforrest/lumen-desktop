"""Provider isolation has real fail-closed subprocess and copy boundaries."""
import ast
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest

from fixture_providers import PROVIDERS, isolate_providers

ROOT = Path(__file__).resolve().parents[2]


class ProviderFixtures(unittest.TestCase):
    def copied(self, base):
        shell = base / 'assets/shell'
        shell.mkdir(parents=True)
        for name, *_ in PROVIDERS:
            shutil.copyfile(ROOT / 'res/lumen-desktop/shell' / name, shell / name)
        return shell

    def test_known_full_shell_fixtures_use_shared_boundary(self):
        fixtures = ('surface_matrix', 'menu_keyboard_input', 'menubar_input',
                    'system_popover_reflow_native', 'fullscreen_menu_native',
                    'installed_switcher_input', 'owned_window_context_input',
                    'application_lifecycle_input', 'smoke_installed', 'search_history_input')
        for name in fixtures:
            with self.subTest(fixture=name):
                source = ROOT / 'tests/desktop' / (name + '.py')
                tree = ast.parse(source.read_text())
                calls = [node for node in ast.walk(tree) if isinstance(node, ast.Call)
                         and isinstance(node.func, ast.Name) and node.func.id == 'isolate_providers']
                self.assertEqual(len(calls), 1)

    def test_actual_status_and_action_rejection(self):
        with tempfile.TemporaryDirectory() as temporary:
            out = Path(temporary)
            shell = self.copied(out)
            before = {p.name: p.read_text() for p in shell.iterdir()}
            records = isolate_providers(shell, out)
            self.assertEqual(len(records), 2)
            for row, (filename, needle, kind, payload) in zip(records, PROVIDERS):
                helper = Path(row['helper'])
                self.assertEqual(row['originalSha256'], hashlib.sha256(before[filename].encode()).hexdigest())
                self.assertEqual((shell / filename).read_text().replace(json.dumps(str(helper)), needle, 1), before[filename])
                for action in ('status', 'disconnect'):
                    command = [sys.executable, str(helper)] + ([] if kind == 'vpn' else [action])
                    result = subprocess.run(command, input=json.dumps({'action': action})+'\n', capture_output=True, text=True, timeout=5)
                    self.assertEqual(result.returncode == 0, action == 'status')
                    if action == 'status':
                        self.assertEqual(json.loads(result.stdout), payload)
                    else:
                        self.assertEqual(result.stdout, '')
                        self.assertIn('Unexpected fixture provider action', result.stderr)
                requests = [json.loads(line) for line in (out / (kind+'-provider-requests.jsonl')).read_text().splitlines()]
                self.assertEqual(requests, [{'action':'status'}, {'action':'disconnect'}])

    def test_changed_seam_cannot_partially_rewrite(self):
        with tempfile.TemporaryDirectory() as temporary:
            out = Path(temporary)
            shell = self.copied(out)
            target = shell / 'SystemState.qml'
            target.write_text('changed provider')
            before = {p.name:p.read_bytes() for p in shell.iterdir()}
            with self.assertRaisesRegex(ValueError, 'seam changed'):
                isolate_providers(shell, out)
            self.assertEqual(before, {p.name:p.read_bytes() for p in shell.iterdir()})
            self.assertFalse((out/'vpn-unavailable-provider.py').exists())

    def test_external_or_symlink_targets_rejected(self):
        with tempfile.TemporaryDirectory() as temporary:
            base = Path(temporary)
            shell = self.copied(base)
            with self.assertRaisesRegex(ValueError, 'private copied shell'):
                isolate_providers(shell, base/'other')
            target = shell/'VpnState.qml'
            original = base/'original.qml'
            target.rename(original)
            target.symlink_to(original)
            with self.assertRaisesRegex(ValueError, 'private regular copy'):
                isolate_providers(shell, base)
            self.assertEqual(original.read_bytes(), (ROOT/'res/lumen-desktop/shell/VpnState.qml').read_bytes())


if __name__ == '__main__':
    unittest.main()

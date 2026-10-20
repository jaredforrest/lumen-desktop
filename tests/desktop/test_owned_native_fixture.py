"""Native runner authority, isolation dispatch and source-change refusal."""
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch
import owned_native_fixture as runner


class NativeRunner(unittest.TestCase):
    def repository(self, root):
        scripts = root / 'tests/desktop'
        (scripts / 'application_fixture').mkdir(parents=True)
        for path in (scripts / 'owned_native_fixture.py', scripts / 'owned_preview_input.py', scripts / 'application_fixture/isolation.py'):
            path.write_text('# synthetic source\n')
        for args in (['init'], ['add', '.'], ['-c', 'user.name=Fixture', '-c', 'user.email=fixture@example.invalid', 'commit', '-m', 'Fixture']):
            subprocess.run(['git', '-c', 'core.hooksPath=/dev/null', *args], cwd=root, check=True, capture_output=True)
        return scripts / 'owned_native_fixture.py'

    def test_unknown_fixture_and_mutable_tools_are_refused(self):
        with self.assertRaises(ValueError):
            runner.command('../foreign', sys.executable, sys.executable)
        with tempfile.NamedTemporaryFile() as tool, self.assertRaises(ValueError):
            runner.command('owned_preview_input', tool.name, sys.executable)

    def exercise(self, mutate=False, dirty=False):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory); script = self.repository(root); out = root.parent / (root.name + '-output')
            if dirty:
                (root / 'pending').write_text('pending')
            argv = ['runner', 'owned_preview_input', '--launcher', sys.executable, '--sway', sys.executable, '--output', str(out)]
            def boundary(source, output, command, **kwargs):
                self.assertNotEqual(source, root); self.assertEqual(output, out)
                self.assertTrue((source / '.git').is_dir())
                self.assertFalse((source / '.git/objects/info/alternates').exists())
                self.assertEqual(subprocess.check_output(['git', '-C', str(source), 'rev-parse', 'HEAD']),
                                 subprocess.check_output(['git', '-C', str(root), 'rev-parse', 'HEAD']))
                self.assertEqual(kwargs, {'software_graphics': False})
                self.assertIn('/artifacts/result', command[2]); self.assertIn('/source/tests/desktop/owned_preview_input.py', command[2])
                if mutate:
                    (root / 'tests/desktop/owned_preview_input.py').write_text('# concurrent edit\n')
                return 7
            try:
                with patch.object(runner, '__file__', str(script)), patch.object(sys, 'argv', argv), patch.object(runner, 'run', side_effect=boundary) as isolated:
                    if dirty or mutate:
                        with self.assertRaises(ValueError): runner.main()
                    else:
                        self.assertEqual(runner.main(), 7)
                    if dirty:
                        isolated.assert_not_called(); self.assertFalse(out.exists())
                    else:
                        record = json.loads((out / 'namespace-provenance.json').read_text())
                        self.assertEqual(record['sourceUnchanged'], not mutate)
                        self.assertEqual(len(record['source']['hashes']), 3)
            finally:
                if out.exists(): __import__('shutil').rmtree(out)

    def test_isolation_dispatch_and_initial_source_are_recorded(self): self.exercise()
    def test_dirty_source_never_starts_namespace(self): self.exercise(dirty=True)
    def test_concurrent_edit_preserves_initial_record_and_fails(self): self.exercise(mutate=True)


if __name__ == '__main__': unittest.main()

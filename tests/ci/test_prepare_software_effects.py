"""Exercise provisioning branches without changing host devices or modules."""
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

SCRIPT = Path(__file__).with_name('prepare-software-effects.sh')


class Provisioning(unittest.TestCase):
    def run_case(self, present, appears=False):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            device = root / 'device'
            if present:
                device.symlink_to('/dev/null')
            calls = root / 'calls'
            sudo = root / 'sudo'
            sudo.write_text('#!/usr/bin/env bash\nprintf "%s\\n" "$*" >> "$CALLS"\n'
                            'if [[ "$1" == modprobe ]]; then\n'
                            '  if [[ "$APPEARS" == 1 ]]; then ln -s /dev/null "$DEVICE"; fi\n'
                            '  exit 1\nfi\nexit 0\n')
            sudo.chmod(0o755)
            script = root / 'prepare.sh'
            script.write_text(SCRIPT.read_text().replace('/dev/udmabuf', str(device)))
            env = dict(os.environ, PATH=f'{root}:{os.environ["PATH"]}', CALLS=str(calls),
                       DEVICE=str(device), APPEARS=str(int(appears)))
            result = subprocess.run(['bash', str(script), str(root / 'artifacts')],
                                    env=env, capture_output=True, text=True)
            self.assertTrue((root / 'artifacts/kernel.txt').is_file())
            return result, calls.read_text() if calls.exists() else ''

    def test_builtin_device_does_not_require_modprobe(self):
        result, calls = self.run_case(True)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertNotIn('modprobe', calls)
        self.assertIn('chmod 0600', calls)

    def test_missing_device_fails_after_attempt(self):
        result, calls = self.run_case(False)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('modprobe udmabuf', calls)
        self.assertNotIn('chmod', calls)

    def test_device_is_authority_even_if_modprobe_exits_nonzero(self):
        result, calls = self.run_case(False, True)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn('chmod 0600', calls)


if __name__ == '__main__':
    unittest.main()

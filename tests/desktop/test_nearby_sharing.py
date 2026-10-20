"""Actual bounded provider processes, schema rejection and disabled defaults."""
import importlib.util
import ctypes
import os
from pathlib import Path
import sys
import tempfile
import signal
import time
import unittest
from unittest.mock import patch

SOURCE = Path(__file__).resolve().parents[2] / 'res/lumen-desktop/runtime/nearby_sharing.py'
spec = importlib.util.spec_from_file_location('nearby', SOURCE)
nearby = importlib.util.module_from_spec(spec)
spec.loader.exec_module(nearby)


class Provider(unittest.TestCase):
    def process(self, source, seconds=2):
        with tempfile.TemporaryDirectory() as directory:
            command = Path(directory) / 'provider'
            command.write_text('#!' + sys.executable + '\nimport os,sys,time\nassert sys.argv[1:]==["status"]\n' + source)
            command.chmod(0o700)
            return nearby.status(command, seconds)

    def test_actual_status_normalizes_only_declared_fields(self):
        value = self.process('print(\'{"available":true,"active":true,"canSend":true,"detail":"On","extra":"not retained"}\')')
        self.assertEqual(value, dict(available=True, active=True, canSend=True, detail='On'))

    def test_missing_send_capability_is_not_inferred(self):
        self.assertFalse(nearby.normalize(dict(available=True, active=False))['canSend'])

    def test_unavailable_and_malformed_states_fail_closed(self):
        for value in ({}, [], dict(available=1, active=False), dict(available=False, active=True),
                      dict(available=False, active=False, canSend=True), dict(available=True, active=False, canSend=1),
                      dict(available=True, active=False, detail='x' * 161), dict(available=True, active=False, detail='line\nline')):
            with self.subTest(value=value), self.assertRaises(ValueError):
                nearby.normalize(value)

    def test_actual_invalid_output_and_unsuccessful_exit(self):
        for source in ('print("not JSON")', 'print("{}")', 'raise SystemExit(3)'):
            with self.subTest(source=source), self.assertRaises(ValueError):
                self.process(source)

    def test_actual_overflow_is_rejected(self):
        with self.assertRaisesRegex(ValueError, 'byte budget'):
            self.process('os.write(1,b"x"*20000)')

    def test_success_cannot_hide_provider_diagnostics(self):
        with self.assertRaisesRegex(ValueError, 'diagnostic'):
            self.process('print(\'{"available":true,"active":false}\');print("Synthetic provider warning",file=sys.stderr)')

    def test_success_retires_descendant_after_leader_exits(self):
        libc = ctypes.CDLL(None, use_errno=True)
        previous = ctypes.c_int()
        self.assertEqual(libc.prctl(37, ctypes.byref(previous), 0, 0, 0), 0)
        self.assertEqual(libc.prctl(36, 1, 0, 0, 0), 0)
        descendant = None
        reaped = False
        temporary = tempfile.TemporaryDirectory()
        pid_file = Path(temporary.name) / 'descendant'
        try:
            source = ('pid=os.fork()\n'
                      'if pid==0:\n'
                      ' os.close(0);os.close(1);os.close(2);time.sleep(10);os._exit(0)\n'
                      'from pathlib import Path\n'
                      'Path(' + repr(str(pid_file)) + ').write_text(str(pid))\n'
                      'print(\'{"available":true,"active":false}\')\n')
            value = self.process(source)
            descendant = int(pid_file.read_text())
            self.assertTrue(value['available'])
            deadline = time.monotonic() + 1
            while time.monotonic() < deadline:
                pid, status = os.waitpid(descendant, os.WNOHANG)
                if pid == descendant:
                    reaped = True
                    self.assertTrue(os.WIFSIGNALED(status))
                    self.assertEqual(os.WTERMSIG(status), signal.SIGKILL)
                    break
                time.sleep(.005)
            self.assertTrue(reaped, 'Owned status descendant survived successful status')
        finally:
            if descendant is None and pid_file.exists():
                descendant = int(pid_file.read_text())
            if descendant is not None and not reaped:
                try:
                    os.kill(descendant, signal.SIGKILL)
                except ProcessLookupError:
                    pass
                os.waitpid(descendant, 0)
            temporary.cleanup()
            self.assertEqual(libc.prctl(36, previous.value, 0, 0, 0), 0)

    def test_eof_cannot_hide_a_still_running_provider(self):
        started = time.monotonic()
        with self.assertRaises((TimeoutError, nearby.subprocess.TimeoutExpired, ValueError)):
            self.process('os.close(1);os.close(2);time.sleep(10)', .2)
        self.assertLess(time.monotonic() - started, 2)

    def test_actual_blocked_output_is_bounded_and_reaped(self):
        started = time.monotonic()
        with self.assertRaises(TimeoutError):
            self.process('time.sleep(10)', .2)
        self.assertLess(time.monotonic() - started, 2)

    def test_unconfigured_default_never_executes(self):
        self.assertIsNone(nearby.configured({}))
        with patch.object(nearby.os, 'environ', {}), patch.object(nearby.sys, 'argv', ['helper', 'open']), patch.object(nearby.os, 'execv') as execute:
            with self.assertRaises(SystemExit):
                nearby.main()
            execute.assert_not_called()
        for path in ('relative', '/tmp/provider', '/nix/store/not-present/bin/provider'):
            with self.subTest(path=path), self.assertRaises(ValueError):
                nearby.configured({'LUMEN_NEARBY_SHARING_PROVIDER': path})

    def test_send_preserves_literal_argument_boundaries(self):
        paths = ['/tmp/a file', '/tmp/$(not-a-command)', '/tmp/é']
        with patch.object(nearby.sys, 'argv', ['helper', 'send', *paths]), patch.object(nearby, 'configured', return_value=Path('/nix/store/provider')), patch.object(nearby, 'status', return_value=dict(canSend=True)), patch.object(nearby.os, 'execv', side_effect=SystemExit) as execute:
            with self.assertRaises(SystemExit):
                nearby.main()
            execute.assert_called_once_with('/nix/store/provider', ['/nix/store/provider', 'send', *paths])

    def test_send_rechecks_capability_and_rejects_relative_or_empty_targets(self):
        with patch.object(nearby.sys, 'argv', ['helper', 'send', '/tmp/file']), patch.object(nearby, 'configured', return_value=Path('/nix/store/provider')), patch.object(nearby, 'status', return_value=dict(canSend=False)), patch.object(nearby.os, 'execv') as execute:
            with self.assertRaises(SystemExit):
                nearby.main()
            execute.assert_not_called()
        for args in (['send'], ['send', 'relative'], ['send'] + ['/tmp/file'] * 129):
            with self.subTest(args=args[:3]), patch.object(nearby.sys, 'argv', ['helper', *args]), self.assertRaises(SystemExit):
                nearby.main()


if __name__ == '__main__':
    unittest.main()

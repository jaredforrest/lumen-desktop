import importlib.util
from pathlib import Path
import subprocess
import types
import unittest

ROOT = Path(__file__).resolve().parents[2]
def load(name):
    spec = importlib.util.spec_from_file_location(name, ROOT / 'res/lumen-desktop/shell' / (name + '.py'))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module
probe = load('biometric_status')
privacy = load('privacy_status')


class Bus:
    def __init__(self):
        self.owner = ':1.2'; self.uid = 0; self.devices = []
        self.calls = []; self.after_devices = lambda: None
    def get_object(self, name, path, introspect):
        assert introspect is False
        self.target = (name, path)
        return self
    def __getattr__(self, method):
        def call(*args, **kwargs):
            self.calls.append((self.target, method, args, kwargs))
            if method == 'GetNameOwner': return self.owner
            if method == 'GetConnectionUnixUser': return self.uid
            if method == 'GetDevices':
                self.after_devices()
                return self.devices
            raise AssertionError('Forbidden method ' + method)
        return call


class Discovery(unittest.TestCase):
    def test_empty_and_reported_never_claim_authentication(self):
        bus = Bus()
        self.assertEqual(probe.discover(bus)['state'], 'unavailable')
        bus.devices = ['/net/reactivated/Fprint/Device/0']
        value = probe.discover(bus)
        self.assertEqual(value['state'], 'reported')
        self.assertIn('does not establish', value['detail'])
        self.assertNotIn('/Device/', str(value))
        for target, method, args, kw in bus.calls:
            self.assertIn(method, ('GetNameOwner', 'GetConnectionUnixUser', 'GetDevices'))
            self.assertGreater(kw['timeout'], 0)
            self.assertLessEqual(kw['timeout'], 1.5)
            if method == 'GetDevices': self.assertEqual(target[0], ':1.2')

    def test_foreign_changed_and_malformed_provider_never_report_reader(self):
        bus = Bus(); bus.uid = 1000
        self.assertEqual(probe.discover(bus)['state'], 'unknown')
        self.assertFalse(any(row[1] == 'GetDevices' for row in bus.calls))
        bus = Bus(); bus.devices = ['/net/reactivated/Fprint/Device/0']
        bus.after_devices = lambda: setattr(bus, 'owner', ':1.3')
        self.assertEqual(probe.discover(bus)['state'], 'unknown')
        for devices in (None, {}, ['bad'], [1], ['/net/reactivated/Fprint/Device/0'] * 2,
                        ['/net/reactivated/Fprint/Device/' + str(n) for n in range(65)]):
            bus = Bus(); bus.devices = devices
            self.assertEqual(probe.discover(bus)['state'], 'unknown')

    def test_late_reply_does_not_publish(self):
        now = [0.0]; bus = Bus()
        bus.after_devices = lambda: now.__setitem__(0, 1.6)
        self.assertEqual(probe.discover(bus, clock=lambda: now[0])['state'], 'unknown')

    def test_outer_process_timeout_and_schema_boundary(self):
        def timeout(*args, **kwargs):
            self.assertEqual(kwargs['timeout'], 2.5)
            raise subprocess.TimeoutExpired(args[0], 2.5)
        self.assertEqual(privacy.biometric_status(timeout)['state'], 'unknown')
        for text in ('null', '[]', 'false', '{}', '{"state":"enabled"}', 'x' * 4097):
            result = privacy.biometric_status(lambda *a, **k: types.SimpleNamespace(stdout=text))
            self.assertEqual(result['state'], 'unknown')


if __name__ == '__main__':
    unittest.main()

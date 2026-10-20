"""Private read-only provider wire acceptance; never connects to the host system bus."""
import argparse
import importlib.util
import json
import os
from pathlib import Path
import select
import subprocess
import sys
import tempfile

from fixture_resources import close_reaped_pipes
from private_native_bus import start_private_bus

if '--serve' in sys.argv:
    import gi
    gi.require_version('Gio', '2.0')
    from gi.repository import Gio, GLib
    root = Path(sys.argv[-1])
    bus = Gio.bus_get_sync(Gio.BusType.SESSION, None)
    service = 'net.reactivated.Fprint'
    bus.call_sync('org.freedesktop.DBus', '/org/freedesktop/DBus', 'org.freedesktop.DBus', 'RequestName', GLib.Variant('(su)', (service, 0)), None, Gio.DBusCallFlags.NONE, 1500, None)
    info = Gio.DBusNodeInfo.new_for_xml('<node><interface name="net.reactivated.Fprint.Manager"><method name="GetDevices"><arg type="ao" direction="out"/></method></interface></node>').interfaces[0]
    pending = []
    def handle(connection, sender, path, interface, method, parameters, invocation):
        with (root / 'calls').open('a') as stream:
            stream.write(method + '\n')
        mode = (root / 'mode').read_text()
        if mode == 'timeout':
            pending.append(invocation)
            return
        if mode == 'denied':
            invocation.return_dbus_error('org.freedesktop.DBus.Error.AccessDenied', 'Synthetic denied discovery')
            return
        if mode == 'retire':
            bus.call_sync('org.freedesktop.DBus', '/org/freedesktop/DBus', 'org.freedesktop.DBus', 'ReleaseName', GLib.Variant('(s)', (service,)), None, Gio.DBusCallFlags.NONE, 1500, None)
        rows = [] if mode == 'empty' else ['/net/reactivated/Fprint/Device/0']
        invocation.return_value(GLib.Variant('(ao)', (rows,)))
    bus.register_object_with_closures2('/net/reactivated/Fprint/Manager', info, handle, None, None)
    print('ready', flush=True)
    GLib.MainLoop().run()
    raise SystemExit

parser = argparse.ArgumentParser()
parser.add_argument('--output', type=Path, required=True)
args = parser.parse_args()
out = args.output.resolve()
out.mkdir(parents=True, exist_ok=False)
source = Path(__file__).resolve().parents[2] / 'res/lumen-desktop/shell/biometric_status.py'
spec = importlib.util.spec_from_file_location('biometric_status', source)
probe = importlib.util.module_from_spec(spec)
spec.loader.exec_module(probe)
processes = []
checks = []
with tempfile.TemporaryDirectory(prefix='lumen-biometric-') as directory:
    root = Path(directory)
    env = dict(os.environ, HOME=directory, XDG_RUNTIME_DIR=directory)
    start_private_bus(env, out, processes)
    import dbus
    bus = dbus.bus.BusConnection(env['DBUS_SESSION_BUS_ADDRESS'])
    def observe():
        # The synthetic provider runs as this user, never masquerades as root.
        return probe.discover(bus, trusted_uid=os.geteuid())
    try:
        assert observe()['value'] == 'Provider inactive'
        checks.append('absent private provider remains inactive without activation')
        (root / 'mode').write_text('empty')
        with (out / 'provider.log').open('w') as log:
            provider = subprocess.Popen([sys.executable, __file__, '--serve', str(root)], env=env, stdout=subprocess.PIPE, stderr=log, text=True)
        processes.append(provider)
        assert select.select([provider.stdout], [], [], 5)[0] and provider.stdout.readline().strip() == 'ready'
        for mode, expected in [('empty', 'unavailable'), ('reader', 'reported'), ('denied', 'unknown'), ('timeout', 'unknown'), ('retire', 'unknown')]:
            (root / 'mode').write_text(mode)
            result = observe()
            assert result['state'] == expected, (mode, result)
            assert '/Device/' not in json.dumps(result)
            checks.append(mode + ' actual provider response classified without authentication claim')
        assert observe()['value'] == 'Provider inactive'
        assert (root / 'calls').read_text().splitlines() == ['GetDevices'] * 5
        checks.append('only five read-only device enumerations; no enrollment or verify calls')
        (out / 'result.json').write_text(json.dumps({'passed': True, 'checks': checks, 'scope': 'Synthetic private provider discovery only; no PAM or biometric authentication acceptance.'}, indent=2) + '\n')
        print('PASS private biometric discovery', len(checks), 'checks')
    finally:
        bus.close()
        for process in reversed(processes):
            if process.poll() is None:
                process.terminate()
                try:
                    process.wait(timeout=3)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait(timeout=3)
        close_reaped_pipes(processes)

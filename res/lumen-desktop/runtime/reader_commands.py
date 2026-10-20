"""Opt-in typed reader commands; no service activation or arbitrary command forwarding."""
import argparse
import json
import os
from pathlib import Path
import sys
import time

COMMANDS = {
    'where-am-i': ('WhereAmIPresenter', 'WhereAmIBasic'),
    'read-all': ('SayAllPresenter', 'SayAll'),
    'stop': ('SpeechManager', 'InterruptSpeech'),
}
SERVICE = 'org.gnome.Orca.Service'
INTERFACE = 'org.gnome.Orca.Module'
BASE = '/org/gnome/Orca/Service/'


class Unavailable(Exception):
    pass


def process_start(pid):
    if type(pid) is not int or pid <= 0:
        raise Unavailable('reader_unavailable')
    return int(Path('/proc/' + str(pid) + '/stat').read_text().rsplit(') ', 1)[1].split()[19])


class ReaderCommands:
    def __init__(self, bus, authority, clock=time.monotonic, identity=process_start):
        self.bus, self.authority, self.clock, self.identity = bus, authority, clock, identity
        self.deadline = clock() + 2.5
        self.dispatched = False
        self.daemon = bus.get_object('org.freedesktop.DBus', '/org/freedesktop/DBus', introspect=False)

    def remaining(self):
        value = self.deadline - self.clock()
        if value <= 0:
            raise Unavailable('provider_timeout')
        return value

    def daemon_call(self, method, *args):
        return getattr(self.daemon, method)(*args, dbus_interface='org.freedesktop.DBus', timeout=self.remaining())

    def owner(self):
        owner = str(self.daemon_call('GetNameOwner', SERVICE))
        if not owner.startswith(':'):
            raise Unavailable('reader_unavailable')
        uid = int(self.daemon_call('GetConnectionUnixUser', owner))
        pid = int(self.daemon_call('GetConnectionUnixProcessID', owner))
        if uid != os.geteuid():
            raise Unavailable('reader_unavailable')
        return owner, uid, pid, self.identity(pid)

    def unlocked(self):
        self.authority.refresh()
        if self.authority.facts != {'known': True, 'locked': False}:
            raise Unavailable('session_unavailable')
        return self.authority.token, self.authority.revision

    def execute(self, command):
        if command not in COMMANDS:
            raise Unavailable('unsupported_command')
        lock = None if command == 'stop' else self.unlocked()
        captured = self.owner()
        module, action = COMMANDS[command]
        target = self.bus.get_object(captured[0], BASE + module, introspect=False)
        available = target.ListCommands(dbus_interface=INTERFACE, timeout=self.remaining())
        if action not in [str(row[0]) for row in available]:
            raise Unavailable('unsupported_command')
        if self.owner() != captured or (command != 'stop' and self.unlocked() != lock):
            raise Unavailable('source_changed')
        # Exact unique owner, fixed module/action, no retries after this boundary.
        remaining = self.remaining()
        self.dispatched = True
        accepted = target.ExecuteCommand(action, False, dbus_interface=INTERFACE, timeout=remaining)
        if self.owner() != captured or (command != 'stop' and self.unlocked() != lock):
            raise Unavailable('dispatch_state_changed')
        if accepted is not True and accepted != 1:
            raise Unavailable('command_rejected')
        return {'accepted': True, 'command': command, 'dispatched': True}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--enabled', action='store_true')
    parser.add_argument('command', choices=tuple(COMMANDS))
    args = parser.parse_args()
    if not args.enabled:
        print(json.dumps({'accepted': False, 'reason': 'disabled', 'dispatched': False}))
        return 1
    address = os.environ.get('DBUS_SESSION_BUS_ADDRESS', '')
    if not address.startswith('unix:') or ';' in address:
        print(json.dumps({'accepted': False, 'reason': 'reader_unavailable', 'dispatched': False}))
        return 1
    sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'notifications'))
    from session_lock import SessionLockAuthority
    import dbus
    authority = SessionLockAuthority()
    client = None
    try:
        bus = dbus.bus.BusConnection(address)
        client = ReaderCommands(bus, authority)
        result = client.execute(args.command)
    except Unavailable as error:
        reason = str(error)
        if client and client.dispatched and reason not in ('dispatch_state_changed', 'command_rejected'):reason = 'dispatch_unknown'
        result = {'accepted': False, 'reason': reason}
    except dbus.DBusException as error:
        reason = 'provider_timeout' if error.get_dbus_name() in ('org.freedesktop.DBus.Error.NoReply', 'org.freedesktop.DBus.Error.Timeout') else 'reader_unavailable'
        if client and client.dispatched:reason = 'dispatch_unknown'
        result = {'accepted': False, 'reason': reason}
    except (OSError, ValueError, TypeError, IndexError):
        result = {'accepted': False, 'reason': 'dispatch_unknown' if client and client.dispatched else 'reader_unavailable'}
    finally:
        authority.close()
    result['dispatched'] = bool(client and client.dispatched)
    print(json.dumps(result))
    return 0 if result['accepted'] else 1


if __name__ == '__main__':
    raise SystemExit(main())

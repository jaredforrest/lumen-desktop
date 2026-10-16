"""Isolated XWayland DBusMenu registrar and JSON bridge for Quickshell."""
import json
import os
import subprocess
import sys
import time
from pathlib import Path
import dbus
import dbus.service
import dbus.mainloop.glib
from gi.repository import GLib

REG = 'com.canonical.AppMenu.Registrar'
PATH = '/com/canonical/AppMenu/Registrar'
MENU = 'com.canonical.dbusmenu'
API = 'org.lumendesktop.GlobalMenuPrototype'


def focused(node):
    if node.get('focused') and node.get('type') in ('con', 'floating_con'):
        return node
    for child in node.get('nodes', []) + node.get('floating_nodes', []):
        result = focused(child)
        if result:
            return result


def clean(layout):
    ident, props, children = layout
    return {'id': int(ident), 'label': str(props.get('label', '')),
            'enabled': bool(props.get('enabled', True)),
            'visible': bool(props.get('visible', True)),
            'separator': props.get('type') == 'separator',
            'toggle': str(props.get('toggle-type', '')),
            'checked': int(props.get('toggle-state', 0)) == 1,
            'shortcut': ' / '.join('+'.join(str(k) for k in seq) for seq in props.get('shortcut', [])),
            'submenu': props.get('children-display') == 'submenu',
            'children': [clean(c) for c in children]}


def lookup(node, ident):
    if not node:
        return None
    if node['id'] == ident:
        return node
    for child in node['children']:
        result = lookup(child, ident)
        if result:
            return result


class Bridge(dbus.service.Object):
    def __init__(self):
        self.bus = dbus.SessionBus()
        self.name = dbus.service.BusName(REG, self.bus, do_not_queue=True)
        super().__init__(self.bus, PATH)
        self.registrations = {}
        self.epoch = 0
        self.state = {'token': '', 'title': '', 'supported': False, 'menu': None, 'windowId': 0, 'appName': 'Desktop'}
        self.pending_action = None
        self.held_token = ""
        self.last_emit = None
        self.last_refresh = 0
        self.bus.add_signal_receiver(self.owner_changed, signal_name='NameOwnerChanged',
                                     dbus_interface='org.freedesktop.DBus')
        self.bus.add_signal_receiver(self.changed, signal_name='LayoutUpdated', dbus_interface=MENU)
        self.bus.add_signal_receiver(self.changed, signal_name='ItemsPropertiesUpdated', dbus_interface=MENU)
        self.pending_refresh = False
        self.events = None
        self.event_buffer = b''
        self.subscribe()
        # Recovery only; normal focus and menu changes are event driven.
        GLib.timeout_add_seconds(2, self.poll)
        self.queue_refresh()
        GLib.io_add_watch(sys.stdin, GLib.IO_IN | GLib.IO_HUP, self.input)
        self.publish()

    def queue_refresh(self):
        if not self.pending_refresh:
            self.pending_refresh = True
            GLib.idle_add(self.refresh_pending)

    def refresh_pending(self):
        self.pending_refresh = False
        try:
            self.refresh()
        except (OSError, subprocess.SubprocessError, ValueError, dbus.DBusException) as ex:
            print(str(ex), file=sys.stderr, flush=True)
        return False

    def subscribe(self):
        if self.events is not None and self.events.poll() is None:
            return
        self.events = subprocess.Popen(
            ['swaymsg', '-m', '-r', '-t', 'subscribe', '["window","workspace"]'],
            stdout=subprocess.PIPE, stderr=sys.stderr)
        os.set_blocking(self.events.stdout.fileno(), False)
        self.event_buffer = b''
        GLib.io_add_watch(self.events.stdout, GLib.IO_IN | GLib.IO_HUP | GLib.IO_ERR, self.focus_event)

    def focus_event(self, source, condition):
        try:
            chunk = os.read(source.fileno(), 65536)
        except BlockingIOError:
            return True
        if not chunk:
            source.close()
            return False
        self.event_buffer += chunk
        while b'\n' in self.event_buffer:
            line, self.event_buffer = self.event_buffer.split(b'\n', 1)
            try:
                event = json.loads(line)
                if event.get('change') in ('focus', 'close', 'new', 'title', 'move', 'empty', 'init'):
                    self.queue_refresh()
            except (ValueError, AttributeError):
                pass
        return True

    def close(self):
        if self.events is not None and self.events.poll() is None:
            self.events.terminate()
            self.events.wait(timeout=3)

    @dbus.service.signal(REG, signature='uso')
    def WindowRegistered(self, window, service, path): pass

    @dbus.service.signal(REG, signature='u')
    def WindowUnregistered(self, window): pass

    @dbus.service.method(REG, in_signature='uo', out_signature='', sender_keyword='sender')
    def RegisterWindow(self, window, path, sender=None):
        self.epoch += 1
        self.registrations[int(window)] = (str(sender), str(path), self.epoch)
        self.WindowRegistered(window, sender, path)
        self.last_refresh = 0
        self.queue_refresh()

    @dbus.service.method(REG, in_signature='u', out_signature='', sender_keyword='sender')
    def UnregisterWindow(self, window, sender=None):
        if self.registrations.get(int(window), (None,))[0] == sender:
            del self.registrations[int(window)]
            self.WindowUnregistered(window)
            self.last_refresh = 0
        self.queue_refresh()

    @dbus.service.method(REG, in_signature='u', out_signature='so')
    def GetMenuForWindow(self, window):
        item = self.registrations.get(int(window))
        return item[:2] if item else ('', '/')

    @dbus.service.method(REG, in_signature='', out_signature='a(uso)')
    def GetMenus(self):
        return [(w, r[0], r[1]) for w, r in self.registrations.items()]

    def owner_changed(self, name, old, new):
        if old and not new:
            for w, r in list(self.registrations.items()):
                if r[0] == str(name):
                    del self.registrations[w]
                    self.WindowUnregistered(w)
            self.last_refresh = 0
            self.queue_refresh()

    def changed(self, *_):
        self.last_refresh = 0
        self.queue_refresh()

    def current(self):
        tree = json.loads(subprocess.check_output(['swaymsg', '-r', '-t', 'get_tree'], timeout=2))
        win = focused(tree)
        def contains_focus(n):
            return n.get('focused', False) or any(contains_focus(c) for c in n.get('nodes', []) + n.get('floating_nodes', []))
        self.output_name = next((o['name'] for o in tree.get('nodes', []) if o.get('type') == 'output' and contains_focus(o)), getattr(self, 'output_name', ''))
        item = self.registrations.get(win.get('window')) if win else None
        if item:
            try:
                owner_pid = int(self.bus.get_object('org.freedesktop.DBus', '/org/freedesktop/DBus').GetConnectionUnixProcessID(item[0], dbus_interface='org.freedesktop.DBus'))
                if owner_pid != win.get('pid'):
                    # Only an exporter helper child may proxy its direct parent window.
                    helper_name = b'export-helper.py'
                    proc = Path('/proc') / str(owner_pid)
                    args = (proc / 'cmdline').read_bytes().split(b'\0')
                    parent = next(int(l.split()[1]) for l in (proc / 'status').read_text().splitlines() if l.startswith('PPid:'))
                    if not any(a.rsplit(b'/', 1)[-1] == helper_name for a in args) or parent != win.get('pid'): item = None
            except (dbus.DBusException, OSError, ValueError, StopIteration):
                item = None
        token = f'{win["id"]}:{win["window"]}:{item[2]}' if item else ''
        return win, item, token

    def proxy(self, item):
        return dbus.Interface(self.bus.get_object(item[0], item[1], introspect=False), MENU)

    def refresh(self):
        win, item, token = self.current()
        # A layer-shell menu temporarily owns keyboard focus. Keep its captured
        # display snapshot, but never relax activation's focused-window guard.
        if not win and self.held_token and self.held_token == self.state['token']:
            parts = self.held_token.split(':')
            reg = self.registrations.get(int(parts[1]))
            if reg and str(reg[2]) == parts[2] and self.bus.name_has_owner(reg[0]):
                try:
                    _, layout = self.proxy(reg).GetLayout(0, -1, dbus.Array([], signature='s'), timeout=2)
                    self.state = dict(self.state, menu=clean(layout))
                    self.last_refresh = time.monotonic()
                    self.publish()
                    return
                except dbus.DBusException:
                    self.held_token = ''
        if win and token != self.held_token:
            self.held_token = ''
        menu = None
        error = ''
        if item:
            try:
                _, layout = self.proxy(item).GetLayout(0, -1, dbus.Array([], signature='s'), timeout=2)
                menu = clean(layout)
            except dbus.DBusException as ex:
                error = ex.get_dbus_name()
        self.state = {'token': token, 'title': win.get('name', '') if win else '',
                      'supported': menu is not None, 'menu': menu, 'error': error,
                      'windowId': win['id'] if win else 0, 'outputName': self.output_name,
                      'appName': (win.get('app_id') or win.get('window_properties', {}).get('class') or 'Application') if win else 'Desktop'}
        self.last_refresh = time.monotonic()
        self.publish()

    def publish(self):
        data = json.dumps(self.state)
        if data != self.last_emit:
            print(data, flush=True)
            self.last_emit = data

    def poll(self):
        try:
            self.subscribe()
            win, _, token = self.current()
            if token != self.state['token'] or (win['id'] if win else 0) != self.state['windowId'] or time.monotonic() - self.last_refresh > 2:
                self.refresh()
        except (OSError, subprocess.SubprocessError, ValueError) as ex:
            print(str(ex), file=sys.stderr, flush=True)
        return True

    def guarded(self, token, read_only=False):
        win, item, current = self.current()
        if read_only and not win and token and token == self.held_token == self.state['token']:
            parts = token.split(':')
            item = self.registrations.get(int(parts[1]))
            if item and str(item[2]) == parts[2] and self.bus.name_has_owner(item[0]):
                return item
        if not token or token != current or token != self.state['token']:
            raise ValueError('Stale or unfocused menu')
        if not self.bus.name_has_owner(item[0]):
            raise ValueError('Exporter disconnected')
        return item

    @dbus.service.method(API, in_signature='', out_signature='s')
    def State(self):
        return json.dumps(self.state)

    @dbus.service.method(API, in_signature='si', out_signature='b')
    def Open(self, token, ident):
        try:
            item = self.guarded(token, read_only=True)
            node = lookup(self.state['menu'], int(ident))
            if not node or not node['enabled'] or not node['visible'] or not node['submenu']:
                return False
            self.proxy(item).AboutToShow(int(ident), timeout=2)
            self.refresh()
            return True
        except (ValueError, dbus.DBusException):
            return False

    @dbus.service.method(API, in_signature='si', out_signature='b')
    def Activate(self, token, ident):
        try:
            item = self.guarded(token)
            self.refresh()
            item = self.guarded(token)
            node = lookup(self.state['menu'], int(ident))
            if not node or not node['enabled'] or not node['visible'] or node['submenu'] or node['separator']:
                return False
            self.proxy(item).Event(int(ident), 'clicked', dbus.Int32(0, variant_level=1), dbus.UInt32(0), timeout=2)
            self.last_refresh = 0
            return True
        except (ValueError, dbus.DBusException):
            return False

    def activate_when_focused(self, token, ident):
        request = (token, ident, time.monotonic() + 0.5)
        self.pending_action = request
        def check():
            if self.pending_action is not request:
                return False
            try:
                win, _, current = self.current()
                if win:
                    self.pending_action = None
                    if current == token:
                        self.refresh()
                        self.Activate(token, ident)
                    return False
                if time.monotonic() >= request[2]:
                    self.pending_action = None
                    return False
            except (OSError, ValueError, subprocess.SubprocessError):
                self.pending_action = None
                return False
            return True
        GLib.timeout_add(16, check)

    def input(self, source, condition):
        if condition & GLib.IO_HUP:
            loop.quit()
            return False
        try:
            cmd = json.loads(source.readline())
            if cmd['action'] == 'hold':
                self.pending_action = None
                if cmd.get('token') == self.state['token']: self.held_token = cmd['token']
            elif cmd['action'] == 'release':
                self.held_token = ''
                self.queue_refresh()
            elif cmd['action'] == 'open': self.Open(cmd['token'], cmd['id'])
            elif cmd['action'] == 'activate': self.Activate(cmd['token'], cmd['id'])
            elif cmd['action'] == 'activate_when_focused': self.activate_when_focused(cmd['token'], cmd['id'])
        except (ValueError, KeyError, TypeError) as ex:
            print(str(ex), file=sys.stderr)
        return True


if __name__ == '__main__':
    if os.environ.get('GLOBAL_MENU_ENABLED') != '1' and os.environ.get('GLOBAL_MENU_PROTOTYPE') != '1':
        raise SystemExit('Use the isolated prototype runner; live installation is not enabled.')
    dbus.mainloop.glib.DBusGMainLoop(set_as_default=True)
    loop = GLib.MainLoop()
    bridge = Bridge()
    try:
        loop.run()
    finally:
        bridge.close()

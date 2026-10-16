"""Notification service and shell transport. Bodies are data, never executable markup."""
import argparse
import json
import os
from pathlib import Path
import re
import shutil
import sys
import time
import threading
from urllib.parse import unquote, urlparse

import dbus
import dbus.service
import dbus.mainloop.glib
from gi.repository import GLib
sys.path.insert(0, str(Path(__file__).resolve().parent.parent/'shell'))
from nix_settings import read_settings, set_section

BUS = 'org.freedesktop.Notifications'
PATH = '/org/freedesktop/Notifications'
UI = 'org.lumendesktop.Notifications'
LIMIT = 100
MAX_ID = 0xffffffff


def local_image(value):
    value = str(value or '')
    if value.startswith('file://'):
        parsed = urlparse(value)
        if parsed.netloc not in ('', 'localhost'):
            return ''
        value = unquote(parsed.path)
    if value.startswith('/') and Path(value).is_file():
        return Path(value).as_uri()
    return ''


def restored_row(value):
    """Only expose the persisted schema; process-owned actions never survive restart."""
    if not isinstance(value, dict):
        raise ValueError('Invalid notification record')
    row = {}
    for key, length in {'app': 256, 'desktop': 256, 'summary': 1024,
                        'body': 8192, 'icon': 16384, 'image': 16384}.items():
        item = value.get(key)
        if not isinstance(item, str) or len(item) > length:
            raise ValueError('Invalid notification text')
        item.encode('utf-8')
        row[key] = item
    for key, minimum, maximum in [('id', 1, MAX_ID), ('timestamp', 0, 8640000000000000), ('urgency', 0, 2)]:
        item = value.get(key)
        if type(item) is not int or not minimum <= item <= maximum:
            raise ValueError('Invalid notification number')
        row[key] = item
    for key in ('resident', 'transient', 'unread'):
        if type(value.get(key)) is not bool:
            raise ValueError('Invalid notification flag')
        row[key] = value[key]
    if row['transient']:
        raise ValueError('Transient notifications are not history')
    # Persisted paths can disappear between sessions. Never load remote image URLs.
    row['icon'] = local_image(row['icon']) or (row['icon'] if re.fullmatch(r'[\w.+-]+', row['icon']) else '')
    row['image'] = local_image(row['image'])
    row.update(active=False, actions=[], banner=False, sender='', deadline=0, bannerDeadline=0)
    return row


def restore_history(file, initial_dnd=False):
    rows, dnd, next_id, invalid = [], initial_dnd, 1, False
    try:
        state = json.loads(file.read_text())
        if not isinstance(state, dict):
            raise ValueError('Invalid notification history')
    except FileNotFoundError:
        return rows, dnd, next_id
    except (ValueError, UnicodeError, RecursionError):
        state, invalid = {}, True
    if type(state.get('dnd', dnd)) is bool:
        dnd = state.get('dnd', dnd)
    else:
        invalid = True
    candidate = state.get('nextId', 1)
    if type(candidate) is int and 1 <= candidate <= MAX_ID:
        next_id = candidate
    else:
        invalid = True
    records = state.get('rows', [])
    if not isinstance(records, list):
        records, invalid = [], True
    seen = set()
    for record in records:
        if len(rows) >= LIMIT:
            break
        try:
            row = restored_row(record)
            if row['id'] in seen:
                raise ValueError('Duplicate notification id')
        except (ValueError, OSError):
            invalid = True
            continue
        seen.add(row['id'])
        rows.append(row)
    if invalid:
        # Keep the original for recovery before save() writes the usable subset.
        backup = file.with_name('history-invalid-%s.json' % time.time_ns())
        # save() collects unused inline images. Preserve companion assets before
        # replacing malformed history, including when its JSON cannot be parsed.
        images = [p for p in file.parent.glob('image-*.png') if p.is_file() and not p.is_symlink()]
        if images:
            image_backup = backup.with_suffix('.images')
            image_backup.mkdir(mode=0o700)
            for image in images:
                copied = image_backup / image.name
                shutil.copy2(image, copied)
                os.chmod(copied, 0o600)
        file.replace(backup)
        os.chmod(backup, 0o600)
        print('Recovered notification history; original saved to ' + str(backup), file=sys.stderr, flush=True)
    return rows, dnd, next_id


class Server(dbus.service.Object):
    def __init__(self, bus, directory):
        self.directory = directory
        directory.mkdir(parents=True, exist_ok=True, mode=0o700)
        os.chmod(directory, 0o700)
        preferences=read_settings()['notifications']
        self.policies=preferences['policies']
        self.file = directory / 'history.json'
        self.rows, self.dnd, self.next_id = restore_history(self.file, os.environ.get("LUMEN_INITIAL_DND") == "1")
        self.bus = bus
        self.name = dbus.service.BusName(BUS, bus, do_not_queue=True)
        super().__init__(bus, PATH)
        bus.add_signal_receiver(self.owner_changed, 'NameOwnerChanged', 'org.freedesktop.DBus')
        self.save()
        self.tick_source = GLib.timeout_add(250, self.tick)

    def save(self):
        rows = [{k: v for k, v in row.items() if k not in ('sender', 'actions', 'banner', 'deadline', 'bannerDeadline', 'active', 'held', 'heldBy', 'delivery')}
                for row in self.rows if not row['transient']]
        data = {'version': 1, 'nextId': self.next_id, 'dnd': self.dnd, 'rows': rows}
        temp = self.file.with_suffix('.tmp')
        with temp.open('w') as stream:
            os.chmod(temp, 0o600)
            json.dump(data, stream, ensure_ascii=False)
            stream.flush()
            os.fsync(stream.fileno())
        temp.replace(self.file)
        directory_fd = os.open(self.directory, os.O_RDONLY | os.O_DIRECTORY)
        try:
            os.fsync(directory_fd)
        finally:
            os.close(directory_fd)
        # Image files have the same retention as their notifications.
        used = {row.get('image', '') for row in self.rows}
        for image in self.directory.glob('image-*.png'):
            if image.as_uri() not in used:
                image.unlink(missing_ok=True)

    def state(self):
        return json.dumps({'ready': True, 'dnd': self.dnd, 'policies':self.policies, 'rows': [{k: v for k, v in r.items() if k != 'sender'} for r in self.rows]}, ensure_ascii=False)

    def changed(self, persist=True):
        if persist:
            self.save()
        self.Changed(self.state())

    def find(self, ident):
        return next((r for r in self.rows if r['id'] == int(ident)), None)

    def close(self, row, reason, keep=False):
        active = row['active']
        row.update(active=False, actions=[], banner=False, deadline=0, bannerDeadline=0)
        if not keep or row['transient']:
            self.rows.remove(row)
        if active:
            self.NotificationClosed(row['id'], reason)

    def owner_changed(self, name, old, new):
        if not str(name).startswith(':') or new:
            return
        altered = False
        for row in self.rows:
            if row.get('heldBy') == name and row.get('held'):
                row['bannerDeadline'] = time.monotonic() + row.pop('held')
                row.pop('heldBy', None)
                altered = True
            if row['sender'] == name and row['actions']:
                row['actions'] = []
                altered = True
        if altered:
            self.changed(False)

    def tick(self):
        now = time.monotonic()
        changed = False
        for row in list(self.rows):
            if row['deadline'] and now >= row['deadline']:
                self.close(row, 1, keep=True)
                changed = True
            elif row['bannerDeadline'] and now >= row['bannerDeadline']:
                row.update(banner=False, bannerDeadline=0)
                changed = True
        if changed:
            self.changed()
        return True

    def image(self, hints, ident):
        raw = hints.get('image-data', hints.get('image_data', hints.get('icon_data')))
        if raw:
            try:
                import gi
                gi.require_version('GdkPixbuf', '2.0')
                from gi.repository import GdkPixbuf
                width, height, stride, alpha, bits, channels, data = raw
                width, height, stride, channels = map(int, (width, height, stride, channels))
                if not (0 < width <= 2048 and 0 < height <= 2048 and bits == 8 and channels == (4 if alpha else 3)
                        and width * channels <= stride <= 16384 and len(data) >= stride * (height - 1) + width * channels
                        and len(data) <= 16 * 1024 * 1024):
                    return ''
                image = GdkPixbuf.Pixbuf.new_from_bytes(GLib.Bytes.new(bytes(data)), GdkPixbuf.Colorspace.RGB, bool(alpha), 8, width, height, stride)
                target = self.directory / ('image-%s-%s.png' % (ident, time.time_ns()))
                image.savev(str(target), 'png', [], [])
                os.chmod(target, 0o600)
                return target.as_uri()
            except (ValueError, TypeError, GLib.Error):
                return ''
        return local_image(hints.get('image-path', hints.get('image_path', '')))

    @dbus.service.method(BUS, in_signature='', out_signature='as')
    def GetCapabilities(self):
        return ['body', 'actions', 'icon-static', 'persistence']

    @dbus.service.method(BUS, in_signature='', out_signature='ssss')
    def GetServerInformation(self):
        return ('Lumen Desktop Notifications', 'lumen-desktop', '1.0', '1.2')

    @dbus.service.method(BUS, in_signature='susssasa{sv}i', out_signature='u', sender_keyword='sender')
    def Notify(self, app_name, replaces_id, app_icon, summary, body, actions, hints, expire_timeout, sender=None):
        row = self.find(replaces_id) if replaces_id else None
        if row and (not row['active'] or row['sender'] != sender):
            row = None
        if row:
            ident = row['id']
            self.rows.remove(row)
        else:
            ident = self.next_id
            while self.find(ident):
                ident = ident % 0xffffffff + 1
            self.next_id = ident % 0xffffffff + 1
        now = time.monotonic()
        urgency = max(0, min(2, int(hints.get('urgency', 1))))
        transient = bool(hints.get('transient', False))
        duration = int(expire_timeout)
        # Normal default banners move to history. Explicit client expiry closes
        # the protocol notification and leaves a non-actionable history entry.
        banner_duration = 0 if duration == 0 or urgency == 2 else (duration / 1000 if duration > 0 else 7)
        deadline = now + duration / 1000 if duration > 0 else now + 7 if transient and duration < 0 else 0
        desktop = str(hints.get('desktop-entry', ''))[:256]
        icon = str(app_icon)[:2048]
        icon = local_image(icon) or (icon if re.fullmatch(r'[\w.+-]+', icon) else '')
        row = dict(id=ident, app=str(app_name)[:256] or desktop or 'Notification', desktop=desktop,
                   summary=str(summary)[:1024], body=str(body)[:8192], icon=icon, image=self.image(hints, ident),
                   actions=[{'id': str(actions[i])[:256], 'label': str(actions[i+1])[:256]} for i in range(0, min(len(actions)-1, 32), 2)],
                   urgency=urgency, resident=bool(hints.get('resident', False)), transient=transient,
                   timestamp=int(time.time()*1000), delivery=str(time.monotonic_ns()), active=True, sender=str(sender), unread=True,
                   banner=not self.dnd, deadline=deadline, bannerDeadline=now+banner_duration if banner_duration else 0)
        policy=self.policies.get(row['desktop'] or row['app'],'banners')
        if policy!='banners':row['banner']=False
        if policy=='off':
            GLib.idle_add(lambda: (self.NotificationClosed(dbus.UInt32(ident), dbus.UInt32(2)), False)[1])
            self.changed()
            return dbus.UInt32(ident)
        self.rows.insert(0, row)
        for old in list(self.rows[LIMIT:]):
            self.close(old, 2)
        # Bound banner count; older ones remain in the centre.
        for old in [r for r in self.rows if r['banner']][3:]:
            old['banner'] = False
        self.changed()
        return dbus.UInt32(ident)

    @dbus.service.method(BUS, in_signature='u', out_signature='', sender_keyword='sender')
    def CloseNotification(self, ident, sender=None):
        row = self.find(ident)
        if row and row['active'] and row['sender'] == sender:
            self.close(row, 3)
            self.changed()

    @dbus.service.signal(BUS, signature='uu')
    def NotificationClosed(self, ident, reason):
        pass

    @dbus.service.signal(BUS, signature='us')
    def ActionInvoked(self, ident, action):
        pass

    @dbus.service.method(UI, in_signature='', out_signature='s')
    def Snapshot(self):
        return self.state()

    @dbus.service.signal(UI, signature='s')
    def Changed(self, state):
        pass

    @dbus.service.method(UI, in_signature='s', out_signature='', sender_keyword='sender')
    def Command(self, encoded, sender=None):
        data = json.loads(encoded)
        if not isinstance(data, dict):
            raise ValueError('Notification command must be an object')
        action = data.get('action')
        if action not in ('hold', 'dnd', 'policy', 'seen', 'dismiss', 'clear', 'hide', 'invoke'):
            raise ValueError('Unknown notification command')
        if 'enabled' in data and type(data['enabled']) is not bool:
            raise ValueError('Notification enabled state must be boolean')
        ident = data.get('id', 0)
        if type(ident) is not int or not 0 <= ident <= MAX_ID:
            raise ValueError('Invalid notification identity')
        row = self.find(ident)
        if action == 'hold' and row:
            if data.get('enabled') and row['bannerDeadline']:
                row['heldBy'] = str(sender)
                row['held'] = max(1, row['bannerDeadline'] - time.monotonic())
                row['bannerDeadline'] = 0
            elif not data.get('enabled') and row.get('held'):
                row['bannerDeadline'] = time.monotonic() + row.pop('held')
                row.pop('heldBy', None)
            return
        if action == 'dnd':
            # Focus mode is live session state, like mute; policies are configuration.
            self.dnd=bool(data['enabled']) if 'enabled' in data else not self.dnd
            if self.dnd:
                for item in self.rows:
                    item['banner'] = False
        elif action == 'policy':
            app,policy=data.get('app'),data.get('policy')
            if not isinstance(app,str) or not app or len(app)>255 or policy not in ('banners','silent','off'):
                raise ValueError('Invalid notification policy')
            preferences=read_settings()['notifications']
            preferences['policies'][app]=policy
            def finish(result):
                self.policies=result['notifications']['policies']
                if policy!='banners':
                    for item in self.rows:
                        if (item['desktop'] or item['app'])==app:item['banner']=False
                self.changed()
                return False
            def rebuild():
                try:
                    result=set_section('notifications',preferences)
                    GLib.idle_add(finish,result)
                except Exception as exc:
                    print('Notification policy was not applied: '+str(exc),file=sys.stderr)
            if os.environ.get('LUMEN_NIX_SETTINGS_BUILD_SOURCE') or Path('/etc/nix/desktop-build-source').exists():
                # Keep notification delivery and D-Bus responsive during a full build.
                threading.Thread(target=rebuild,daemon=True).start()
                return
            finish(set_section('notifications',preferences))
        elif action == 'seen':
            for item in self.rows:
                item.update(unread=False, banner=False)
        elif action == 'dismiss' and row:
            self.close(row, 2)
        elif action == 'clear':
            group = data.get('group')
            for item in list(self.rows):
                if group is None or (item['desktop'] or item['app']) == group:
                    self.close(item, 2)
        elif action == 'hide' and row:
            row['banner'] = False
        elif action == 'invoke' and row and row['active']:
            # A replacement keeps the protocol ID but owns a new action set.
            if 'delivery' in data and data['delivery'] != row.get('delivery'):
                return
            key = str(data.get('key', 'default'))
            if key in [a['id'] for a in row['actions']] and self.bus.name_has_owner(row['sender']):
                self.ActionInvoked(row['id'], key)
                if not row['resident']:
                    self.close(row, 2)
        self.changed()


def main():
    dbus.mainloop.glib.DBusGMainLoop(set_as_default=True)
    bus = dbus.SessionBus()
    mode = sys.argv[1] if len(sys.argv) > 1 else 'serve'
    if mode == 'serve':
        state = Path(os.environ.get('LUMEN_NOTIFICATION_STATE', str(Path(os.environ.get('XDG_STATE_HOME', str(Path.home()/'.local/state'))) / 'lumen-desktop/notifications')))
        server = Server(bus, state)
        GLib.MainLoop().run()
    else:
        api = dbus.Interface(bus.get_object(BUS, PATH), UI)
        if mode == 'watch':
            def output(state):
                print(str(state), flush=True)
            api.connect_to_signal('Changed', output)
            output(api.Snapshot())
            pending = bytearray()
            loop = GLib.MainLoop()
            def command(source, condition):
                chunk = os.read(sys.stdin.fileno(), 65536)
                if not chunk:
                    loop.quit()
                    return False
                pending.extend(chunk)
                while b'\n' in pending:
                    line, _, rest = pending.partition(b'\n')
                    pending[:] = rest
                    try:
                        api.Command(line.decode('utf-8'))
                    except (dbus.DBusException, ValueError):
                        print(json.dumps({'error':'Notification request failed'}), flush=True)
                return True
            def owner_changed(name, old, new):
                if name == BUS and old:
                    loop.quit()
            bus.add_signal_receiver(owner_changed, 'NameOwnerChanged', 'org.freedesktop.DBus')
            GLib.io_add_watch(sys.stdin, GLib.IO_IN | GLib.IO_HUP, command)
            loop.run()
        elif mode == 'dnd':
            api.Command(json.dumps({'action':'dnd'}))
        elif mode == 'mode':
            print('do-not-disturb' if json.loads(api.Snapshot())['dnd'] else 'default')
        elif mode == 'status':
            print(api.Snapshot())
        else:
            raise SystemExit('Unknown command')

if __name__ == '__main__':
    main()

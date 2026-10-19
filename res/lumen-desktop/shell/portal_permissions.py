"""Inspect supported portal decisions without exposing restore tokens or metadata."""
import hashlib
import json
import sys

NAME = 'org.freedesktop.impl.portal.PermissionStore'
PATH = '/org/freedesktop/impl/portal/PermissionStore'
INTERFACE = NAME
CATALOGUE = (
    ('devices', 'camera', 'Camera'),
    ('screenshot', 'screenshot', 'Screenshots'),
    ('screencast', None, 'Saved screen sharing'),
    ('remote-desktop', None, 'Saved remote desktop'),
)
SCOPE = ('These are saved portal decisions, not a list of active capture. '
         'Native apps can have access outside portals. Microphone access is not managed here.')


def digest(value):
    return hashlib.sha256(value.encode()).hexdigest()


def identity(table, resource, app):
    return digest(json.dumps([table, resource, app], ensure_ascii=False))


def decision_revision(owner, row_id, permissions):
    return digest(json.dumps([owner, row_id, permissions], ensure_ascii=False))


def safe_name(value):
    return isinstance(value, str) and 0 < len(value) <= 512 and not any(ord(c) < 32 or ord(c) == 127 for c in value)


def app_label(app):
    # A friendly name never replaces the exact provider app ID in confirmation.
    try:
        import gi
        gi.require_version('GioUnix', '2.0')
        from gi.repository import GioUnix
        entry = GioUnix.DesktopAppInfo.new(app + '.desktop')
        label = entry.get_display_name() if entry else app
        return label if safe_name(label) else app
    except (ImportError, ValueError, TypeError):
        return app


class ProviderError(Exception):
    pass


class Provider:
    def __init__(self, bus=None):
        import dbus
        self.dbus = dbus
        self.bus = bus or dbus.SessionBus()
        proxy = self.bus.get_object(NAME, PATH, introspect=False)
        self.version = int(dbus.Interface(proxy, 'org.freedesktop.DBus.Properties').Get(INTERFACE, 'version', timeout=4))
        self.owner = str(self.bus.get_name_owner(NAME))
        # Pin each snapshot/action to this connection owner, never a replacement.
        self.store = dbus.Interface(self.bus.get_object(self.owner, PATH, introspect=False), INTERFACE)

    def call(self, method, *args):
        return getattr(self.store, method)(*args, timeout=4)

    def entries(self):
        rows, targets, coverage = [], {}, []
        for table, fixed_resource, capability in CATALOGUE:
            try:
                resources = [fixed_resource] if fixed_resource else list(self.call('List', table))
                count = 0
                scoped_rows, scoped_targets = [], {}
                for resource in resources:
                    try:
                        permissions, _metadata = self.call('Lookup', table, resource)
                    except self.dbus.DBusException as error:
                        if error.get_dbus_name() == 'org.freedesktop.portal.Error.NotFound':
                            continue
                        raise
                    for app, values in permissions.items():
                        app, values = str(app), [str(value) for value in values]
                        row_id = identity(table, str(resource), app)
                        known = len(values) == 1 and values[0] in ('yes', 'no', 'ask')
                        named = safe_name(app)
                        saved = fixed_resource is None
                        actions = []
                        if named and known:
                            if not saved and values != ['no']:
                                actions.append('deny')
                            if self.version >= 2:
                                actions.append('forget' if saved else 'reset')
                        resource_label = ('Session ' + digest(str(resource))[:16]) if saved else fixed_resource
                        label = {'yes': 'Allowed', 'no': 'Denied', 'ask': 'Ask'}.get(values[0], 'Unrecognised decision') if known else 'Unrecognised decision'
                        scoped_rows.append(dict(id=row_id, appId=app if named else '', appName=app_label(app) if named else 'Unidentified portal app',
                                         capability=capability, resource=resource_label, decision=label,
                                         revision=decision_revision(self.owner, row_id, values), actions=actions,
                                         detail=('Forgets this saved session only. It does not stop active sharing.' if saved else
                                                 'Applies to future portal requests. Existing device access may continue.') if named and known else
                                                 'Unrecognised provider format. This entry is read-only.' if named else
                                                 'The provider has no distinct app ID. This entry cannot safely identify one client.'))
                        scoped_targets[row_id] = (table, str(resource), app, values)
                        count += 1
                rows.extend(scoped_rows)
                targets.update(scoped_targets)
                coverage.append(dict(capability=capability, state='available', count=count))
            except self.dbus.DBusException as error:
                # A missing table is an empty supported scope, not a failed bus.
                if error.get_dbus_name() == 'org.freedesktop.portal.Error.NotFound':
                    coverage.append(dict(capability=capability, state='available', count=0))
                else:
                    coverage.append(dict(capability=capability, state='unavailable', count=0))
        return sorted(rows, key=lambda row: (row['capability'], row['appId'], row['resource'])), targets, coverage

    def snapshot(self):
        rows, _, coverage = self.entries()
        return dict(available=True, version=self.version, rows=rows, coverage=coverage, scope=SCOPE)

    def apply(self, request):
        if set(request) != {'action', 'id', 'revision', 'confirmed'} or request['confirmed'] is not True:
            raise ProviderError('An explicit app and resource confirmation is required.')
        rows, targets, _ = self.entries()
        row = next((item for item in rows if item['id'] == request['id']), None)
        if not row or row['revision'] != request['revision']:
            raise ProviderError('The saved decision changed. Refresh and confirm its current app and resource.')
        action = request['action']
        if action not in row['actions']:
            raise ProviderError('This action is not supported for this saved decision.')
        table, resource, app, expected = targets[row['id']]
        # A second lookup catches changes between enumeration and confirmation.
        current, _metadata = self.call('Lookup', table, resource)
        if app not in current or [str(value) for value in current[app]] != expected:
            raise ProviderError('The saved decision changed. Refresh before trying again.')
        if action == 'deny':
            self.call('SetPermission', table, False, resource, app, self.dbus.Array(['no'], signature='s'))
        else:
            self.call('DeletePermission', table, resource, app)
        latest = self.snapshot()
        updated = next((item for item in latest['rows'] if item['id'] == row['id']), None)
        if action == 'deny' and (not updated or updated['decision'] != 'Denied') or action != 'deny' and updated:
            raise ProviderError('The provider did not confirm the requested change. Refresh to inspect its current state.')
        latest['message'] = ('Portal decision denied.' if action == 'deny' else 'Saved portal decision removed.') + ' Existing capture may continue; stop it in the sharing app.'
        return latest


def handle(request, provider_factory=Provider):
    try:
        provider = provider_factory()
        return provider.snapshot() if request.get('action') == 'list' else provider.apply(request)
    except ProviderError as error:
        return dict(available=False, rows=[], scope=SCOPE, error=str(error))
    except Exception as error:
        # Provider errors may include resource tokens or private metadata.
        import dbus
        if isinstance(error, dbus.DBusException):
            name = error.get_dbus_name()
            message = 'Permission provider denied this request.' if name in ('org.freedesktop.DBus.Error.AccessDenied', 'org.freedesktop.portal.Error.NotAllowed') else 'The portal permission provider is unavailable or did not complete the request.'
            return dict(available=False, rows=[], scope=SCOPE, error=message)
        raise


if __name__ == '__main__':
    try:
        request = json.loads(sys.stdin.readline())
        if not isinstance(request, dict):
            raise ValueError('Expected an object')
        print(json.dumps(handle(request), ensure_ascii=False))
    except (ValueError, TypeError, KeyError):
        print(json.dumps(dict(available=False, rows=[], scope=SCOPE, error='Invalid permission request.')))

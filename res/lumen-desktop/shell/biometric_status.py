"""Read-only reader discovery; never enrolls, verifies, claims or activates devices."""
import json
import time

SERVICE = 'net.reactivated.Fprint'
MANAGER = '/net/reactivated/Fprint/Manager'
INTERFACE = 'net.reactivated.Fprint.Manager'
DAEMON = 'org.freedesktop.DBus'
DETAIL = 'Enrollment and authentication policy are not assessed. Password authentication remains managed by the host.'


def fact(state, value, detail):
    return {'state': state, 'value': value, 'detail': detail}


def discover(bus, clock=time.monotonic, trusted_uid=0):
    """An inactive provider is unknown, not proof that hardware is absent."""
    deadline = clock() + 1.5
    def call(name, path, interface, method, *args):
        remaining = deadline - clock()
        if remaining <= 0:
            raise TimeoutError()
        target = bus.get_object(name, path, introspect=False)
        result = getattr(target, method)(*args, dbus_interface=interface, timeout=remaining)
        if clock() >= deadline:
            raise TimeoutError()
        return result
    def daemon(method, *args):
        return call(DAEMON, '/org/freedesktop/DBus', DAEMON, method, *args)
    try:
        owner = str(daemon('GetNameOwner', SERVICE))
        if not owner.startswith(':') or int(daemon('GetConnectionUnixUser', owner)) != trusted_uid:
            raise ValueError('Untrusted owner')
        # A unique destination cannot auto-start a replacement provider.
        devices = call(owner, MANAGER, INTERFACE, 'GetDevices')
        if (not isinstance(devices, (list, tuple)) or len(devices) > 64
                or any(not isinstance(p, str) or len(p) > 256 or not p.startswith('/net/reactivated/Fprint/Device/') for p in devices)
                or len(set(devices)) != len(devices)):
            raise ValueError('Invalid device list')
        if str(daemon('GetNameOwner', SERVICE)) != owner:
            raise ValueError('Provider changed')
        if not devices:
            return fact('unavailable', 'No reader reported', 'The active fingerprint provider reports no reader. Other biometric providers are not assessed. ' + DETAIL)
        return fact('reported', 'Reader reported', 'The active fingerprint provider reports a reader; this does not establish that biometric unlock is configured or working. ' + DETAIL)
    except Exception as error:
        # No raw exception text, device paths, identities or enrollment data leave this probe.
        name = getattr(error, 'get_dbus_name', lambda: '')()
        if name == 'org.freedesktop.DBus.Error.NameHasNoOwner':
            return fact('unknown', 'Provider inactive', 'Fingerprint status is unavailable while its provider is inactive. This check does not start it. ' + DETAIL)
        return fact('unknown', 'Unavailable', 'Fingerprint reader status could not be established. ' + DETAIL)


def main():
    bus = None
    try:
        import dbus
        bus = dbus.SystemBus(private=True)
        result = discover(bus)
    except Exception:
        result = fact('unknown', 'Unavailable', 'Fingerprint reader status could not be established. ' + DETAIL)
    finally:
        if bus is not None:
            bus.close()
    print(json.dumps(result))


if __name__ == '__main__':
    main()

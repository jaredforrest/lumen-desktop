"""Read an authoritative aggregate power snapshot without starting services."""
import math
import dbus

SERVICE = 'org.freedesktop.UPower'
ROOT = '/org/freedesktop/UPower'
DEVICE = 'org.freedesktop.UPower.Device'
PROPERTIES = 'org.freedesktop.DBus.Properties'
STATES = {0: 'unknown', 1: 'charging', 2: 'discharging', 3: 'empty',
          4: 'charged', 5: 'pending-charge', 6: 'pending-discharge'}


def unavailable():
    return dict(available=False, present=False, percentage=None,
                state='unavailable', low=False, secondsRemaining=None)


def number(value):
    if isinstance(value, (bool, dbus.Boolean)) or not isinstance(value, (int, float)):
        raise ValueError('Invalid power number')
    if not math.isfinite(value):
        raise ValueError('Invalid power number')
    return float(value)


def normalize(properties):
    """UPower percentages use 0..100, never an absent-device zero placeholder."""
    result = unavailable()
    present = properties.get('IsPresent')
    if not isinstance(present, (bool, dbus.Boolean)):
        raise ValueError('Invalid presence flag')
    if not present:
        return dict(result, available=True, state='not-present')
    kind = number(properties.get('Type'))
    supply = properties.get('PowerSupply')
    if not isinstance(supply, (bool, dbus.Boolean)):
        raise ValueError('Invalid supply flag')
    if kind not in (2, 3) or not supply:
        return dict(result, available=True, state='not-present')
    percentage = number(properties.get('Percentage'))
    state = number(properties.get('State'))
    if not 0 <= percentage <= 100 or state not in STATES:
        raise ValueError('Invalid power state')
    field = 'TimeToFull' if state == 1 else 'TimeToEmpty' if state == 2 else None
    remaining = number(properties.get(field, 0)) if field else 0
    if remaining < 0:
        raise ValueError('Invalid power estimate')
    return dict(available=True, present=True, percentage=percentage,
                state=STATES[state], low=state in (2, 3, 6) and percentage <= 20,
                secondsRemaining=remaining if remaining > 0 else None)


def status(bus=None):
    try:
        bus = bus if bus is not None else dbus.SystemBus()
        # Address the unique owner throughout, so replacement cannot splice facts
        # from two daemon instances or trigger D-Bus service activation.
        owner = bus.get_name_owner(SERVICE)
        root = bus.get_object(owner, ROOT, introspect=False)
        path = dbus.Interface(root, SERVICE).GetDisplayDevice(timeout=2)
        device = bus.get_object(owner, str(path), introspect=False)
        properties = dbus.Interface(device, PROPERTIES).GetAll(DEVICE, timeout=2)
        result = normalize(properties)
        if bus.get_name_owner(SERVICE) != owner:
            return unavailable()
        return result
    except (dbus.DBusException, OSError, TypeError, ValueError):
        return unavailable()

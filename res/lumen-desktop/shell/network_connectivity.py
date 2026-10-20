"""Read existing per-device connectivity facts; never initiate network checks."""
import dbus
import time

SERVICE='org.freedesktop.NetworkManager'
ROOT='/org/freedesktop/NetworkManager'
DEVICE=SERVICE+'.Device'
PROPERTIES='org.freedesktop.DBus.Properties'

def number(value):
    if isinstance(value,(bool,dbus.Boolean)) or not isinstance(value,(int,dbus.UInt32,dbus.Int32)):raise ValueError('Invalid network state')
    return int(value)

def flag(value):
    if not isinstance(value,(bool,dbus.Boolean)):raise ValueError('Invalid network flag')
    return bool(value)

def normalize(manager,device):
    state=number(device.get('State'))
    if not flag(device.get('Managed')):return None
    enabled=flag(manager.get('WirelessEnabled')) and flag(manager.get('WirelessHardwareEnabled'))
    if not enabled:return dict(phase='disabled',connectivity='unknown')
    if state in (40,50,60):phase='associating'
    elif state in (70,80,90):phase='acquiring'
    elif state==100:phase='connected'
    elif state==110:phase='disconnecting'
    elif state in (30,120):phase='disconnected'
    elif state==20:phase='unavailable'
    elif state in (0,10):return None
    else:raise ValueError('Invalid device state')
    connectivity='unknown'
    if phase=='connected' and flag(manager.get('ConnectivityCheckAvailable',False)) and flag(manager.get('ConnectivityCheckEnabled',False)):
        values=[number(device.get(key,0)) for key in ('Ip4Connectivity','Ip6Connectivity')]
        if any(value not in range(5) for value in values):raise ValueError('Invalid connectivity state')
        # A successful family can provide access even if the other is limited.
        if 4 in values:connectivity='full'
        elif 2 in values:connectivity='portal'
        elif 3 in values:connectivity='limited'
        elif all(value==1 for value in values):connectivity='none'
    return dict(phase=phase,connectivity=connectivity)

def status(interface=None,bus=None):
    try:
        bus=bus if bus is not None else dbus.SystemBus()
        deadline=time.monotonic()+3
        def timeout():
            remaining=deadline-time.monotonic()
            if remaining<=0:raise ValueError('Network snapshot expired')
            return min(1,remaining)
        def current_owner():
            return bus.call_blocking('org.freedesktop.DBus','/org/freedesktop/DBus','org.freedesktop.DBus','GetNameOwner','s',(SERVICE,),timeout=timeout())
        owner=current_owner()
        manager=bus.get_object(owner,ROOT,introspect=False)
        props=dbus.Interface(manager,PROPERTIES).GetAll(SERVICE,timeout=timeout())
        paths=dbus.Interface(manager,SERVICE).GetDevices(timeout=timeout())
        if len(paths)>128:raise ValueError('Too many network devices')
        candidates=[]
        for path in paths:
            obj=bus.get_object(owner,str(path),introspect=False)
            device=dbus.Interface(obj,PROPERTIES).GetAll(DEVICE,timeout=timeout())
            if number(device.get('DeviceType'))!=2:continue
            name=str(device.get('Interface',''))
            if interface and name!=interface:continue
            candidates.append((name,device))
        if len(candidates)!=1:return None
        name,device=candidates[0];result=normalize(props,device)
        if current_owner()!=owner:return None
        return dict(result,interface=name,source='network-manager') if result else None
    except (dbus.DBusException,OSError,TypeError,ValueError):return None

def merge(wireless,report):
    result=dict(wireless)
    result['controlAvailable']=bool(wireless.get('available'))
    state=wireless.get('state','UNKNOWN')
    phase={'COMPLETED':'connected','DISCONNECTED':'disconnected','INACTIVE':'disconnected','INTERFACE_DISABLED':'disabled','SCANNING':'associating','ASSOCIATING':'associating','ASSOCIATED':'associating','AUTHENTICATING':'associating','4WAY_HANDSHAKE':'associating','GROUP_HANDSHAKE':'associating'}.get(state,'unknown')
    result.update(phase=phase if wireless.get('available') else 'service-unavailable',connectivity='unknown',statusSource='supplicant' if wireless.get('available') else 'unavailable')
    if report:
        result.update(phase=report['phase'],connectivity=report['connectivity'],statusSource=report['source'],interface=report['interface'])
    return result

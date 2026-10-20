"""Actual private D-Bus power provider: transitions, loss and replacement."""
import argparse
from contextlib import ExitStack
import json
import os
import tempfile
from private_native_bus import start_private_bus
from pathlib import Path
import subprocess as sp
import sys
import time

parser=argparse.ArgumentParser()
parser.add_argument('--serve',action='store_true')
parser.add_argument('--output',type=Path)
args=parser.parse_args()


def serve():
    import dbus
    import dbus.service
    from dbus.mainloop.glib import DBusGMainLoop
    from gi.repository import GLib
    DBusGMainLoop(set_as_default=True)
    bus=dbus.bus.BusConnection(os.environ['DBUS_SYSTEM_BUS_ADDRESS'])
    name=dbus.service.BusName('org.freedesktop.UPower',bus=bus,do_not_queue=True)
    class Provider(dbus.service.Object):
        case='discharging'
        @dbus.service.method('org.freedesktop.UPower',out_signature='o')
        def GetDisplayDevice(self):return '/org/freedesktop/UPower/devices/DisplayDevice'
        @dbus.service.method('org.freedesktop.DBus.Properties',in_signature='s',out_signature='a{sv}')
        def GetAll(self,interface):
            assert interface=='org.freedesktop.UPower.Device'
            state,percentage={'charging':(1,42),'discharging':(2,42),'low':(2,9),'charged':(4,100),'empty':(3,0),'absent':(0,0),'invalid':(2,200),'owner-loss':(2,42)}[self.case]
            values=dict(IsPresent=dbus.Boolean(self.case!='absent'),PowerSupply=dbus.Boolean(True),Type=dbus.UInt32(2),Percentage=dbus.Double(percentage),State=dbus.UInt32(state),TimeToEmpty=dbus.Int64(5400),TimeToFull=dbus.Int64(1200))
            if self.case=='owner-loss':bus.release_name('org.freedesktop.UPower')
            return values
        @dbus.service.method('org.lumen.TestPower',in_signature='s')
        def SetCase(self,value):Provider.case=str(value)
    root=Provider(bus,'/org/freedesktop/UPower')
    device=Provider(bus,'/org/freedesktop/UPower/devices/DisplayDevice')
    print('READY',flush=True)
    GLib.MainLoop().run()


if args.serve:
    serve()
    raise SystemExit

assert args.output is not None,'--output is required'
args.output.mkdir(parents=True,exist_ok=False)
processes=[]
def reap():
    for process in reversed(processes):
        if process.poll() is None:
            process.terminate()
            try:process.wait(timeout=5)
            except sp.TimeoutExpired:process.kill();process.wait(timeout=5)
        if process.stdout and not process.stdout.closed:process.stdout.close()

with ExitStack() as stack:
    stack.callback(reap)
    private=stack.enter_context(tempfile.TemporaryDirectory(prefix='lumen-power-bus-'))
    environment=dict(os.environ,HOME=private,XDG_RUNTIME_DIR=private,
                     DBUS_SESSION_BUS_ADDRESS='unix:path='+private+'/absent-session',
                     DBUS_SYSTEM_BUS_ADDRESS='unix:path='+private+'/absent-system')
    for key in ('CONFIG','STATE','CACHE','DATA'):environment['XDG_'+key+'_HOME']=str(Path(private)/key.lower())
    daemon=start_private_bus(environment,args.output,processes)
    address=environment.pop('DBUS_SESSION_BUS_ADDRESS')
    environment['DBUS_SYSTEM_BUS_ADDRESS']=address
    import dbus
    bus=dbus.bus.BusConnection(address)
    activatable=bus.call_blocking('org.freedesktop.DBus','/org/freedesktop/DBus','org.freedesktop.DBus','ListActivatableNames','',())
    assert set(map(str,activatable))=={'org.freedesktop.DBus'},'Private power bus must not activate host services'
    def wait(predicate):
        deadline=time.monotonic()+5
        while time.monotonic()<deadline:
            if predicate():return
            time.sleep(.02)
        raise AssertionError('Private power provider did not become ready')
    def launch(index):
        provider=sp.Popen([sys.executable,__file__,'--serve'],env=environment,stdout=stack.enter_context((args.output/('provider-'+str(index)+'.log')).open('w')),stderr=sp.STDOUT)
        processes.append(provider)
        wait(lambda:bus.name_has_owner('org.freedesktop.UPower'))
        return provider
    source=Path(__file__).resolve().parents[2]/'res/lumen-desktop/shell'
    environment['PYTHONPATH']=str(source)
    def snapshot():
        return json.loads(sp.check_output([sys.executable,'-c','import json,battery_power;print(json.dumps(battery_power.status()))'],env=environment,text=True,timeout=8))
    def select(case):
        obj=bus.get_object('org.freedesktop.UPower','/org/freedesktop/UPower',introspect=False)
        dbus.Interface(obj,'org.lumen.TestPower').SetCase(case,timeout=2)
    results=[]
    try:
        provider=launch(1)
        for case in ('charging','discharging','low','charged','empty','absent','invalid'):
            select(case);value=snapshot()
            if case=='absent':assert value['available'] and not value['present'] and value['percentage'] is None
            elif case=='invalid':assert not value['available'] and value['percentage'] is None
            else:
                assert value['present'] and value['state']=={'low':'discharging'}.get(case,case)
                assert value['percentage']=={'charging':42,'discharging':42,'low':9,'charged':100,'empty':0}[case]
                assert value['low']==(case in ('low','empty'))
            results.append(dict(case=case,status=value))
        select('owner-loss');value=snapshot();assert not value['available'] and value['percentage'] is None
        results.append(dict(case='owner-lost-during-reply',status=value))
        provider.terminate();provider.wait(timeout=5)
        wait(lambda:not bus.name_has_owner('org.freedesktop.UPower'))
        value=snapshot();assert not value['available'] and value['percentage'] is None
        results.append(dict(case='provider-absent',status=value))
        replacement=launch(2);select('charging');value=snapshot();assert value['state']=='charging' and value['percentage']==42
        results.append(dict(case='provider-replaced',status=value))
        (args.output/'result.json').write_text(json.dumps(dict(passed=True,privateSystemBus=True,activationFree=True,explicitBusConfiguration=True,scope='Real D-Bus facts and service lifecycle; no physical battery acceptance',checks=results),indent=2)+'\n')
        print('PASS private power provider transitions, owner loss and replacement')
    finally:
        reap()

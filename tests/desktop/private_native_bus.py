"""Activation-free private session bus for disposable native fixtures."""
import select,subprocess as sp
from pathlib import Path

def start_private_bus(env,output,processes):
    runtime=Path(env['XDG_RUNTIME_DIR'])
    env['DBUS_SYSTEM_BUS_ADDRESS']='unix:path='+str(runtime/'no-system-bus')
    config=Path(output)/'bus.conf'
    config.write_text('<busconfig><type>session</type><listen>unix:tmpdir='+str(runtime)+'</listen><policy context="default"><allow own="*"/><allow send_destination="*"/><allow receive_sender="*"/></policy></busconfig>')
    with (Path(output)/'bus.log').open('w') as log:
        bus=sp.Popen(['dbus-daemon','--nofork','--print-address=1','--config-file='+str(config)],env=env,stdout=sp.PIPE,stderr=log,text=True,start_new_session=True)
    processes.append(bus)
    assert select.select([bus.stdout],[],[],5)[0], 'Private bus failed to start'
    address=bus.stdout.readline().strip()
    assert address.startswith('unix:'), 'Private bus did not publish an address'
    env['DBUS_SESSION_BUS_ADDRESS']=address
    return bus

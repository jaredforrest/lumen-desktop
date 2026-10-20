"""Explicit private standard chooser providers for installed application tests."""
import hashlib,json
from pathlib import Path

def start(environment,manifest,launch,wait):
 import dbus
 path=Path(manifest).resolve(strict=True)
 if not str(path).startswith('/nix/store/'):raise ValueError('Chooser manifest must be immutable')
 tools=json.loads(path.read_text());facts={'manifest':str(path),'sha256':hashlib.sha256(path.read_bytes()).hexdigest(),'providers':{}}
 for key in ('bus','portal','backend'):
  binary=Path(tools[key]).resolve(strict=True)
  if not str(binary).startswith('/nix/store/') or not binary.is_file():raise ValueError('Chooser provider must be immutable')
  facts['providers'][key]={'path':str(binary),'sha256':hashlib.sha256(binary.read_bytes()).hexdigest()}
 data=Path(tools['backendData']).resolve(strict=True)
 if not str(data).startswith('/nix/store/') or not data.is_dir():raise ValueError('Chooser data must be immutable')
 config=Path(environment['XDG_CONFIG_HOME'])/'xdg-desktop-portal';config.mkdir(parents=True,exist_ok=True)
 (config/'portals.conf').write_text('[preferred]\ndefault=none\norg.freedesktop.impl.portal.FileChooser=gtk\n')
 bus_config=Path('/run/fixture/chooser-bus.conf');address='unix:path=/run/fixture/chooser-bus'
 bus_config.write_text('<busconfig><type>session</type><listen>'+address+'</listen><auth>EXTERNAL</auth><policy context="default"><allow own="*"/><allow send_destination="*"/><allow receive_sender="*"/></policy></busconfig>')
 bus=launch([tools['bus'],'--nofork','--config-file='+str(bus_config)],'chooser-bus')
 wait(lambda:Path('/run/fixture/chooser-bus').is_socket(),'owned chooser bus',bus)
 icon_dirs=[Path(value).resolve(strict=True) for value in tools['iconData']]
 if any(not str(value).startswith('/nix/store/') or not value.is_dir() for value in icon_dirs):raise ValueError('Chooser icon data must be immutable')
 environment.update(DBUS_SESSION_BUS_ADDRESS=address,XDG_DATA_DIRS=':'.join(map(str,[data,*icon_dirs])),XDG_CURRENT_DESKTOP='GNOME',GIO_USE_VFS='local',FONTCONFIG_FILE=tools['fontConfig'],GSETTINGS_BACKEND='memory',GTK_USE_PORTAL='0')
 connection=dbus.bus.BusConnection(address)
 backend=launch([tools['backend']],'chooser-backend')
 wait(lambda:connection.name_has_owner('org.freedesktop.impl.portal.desktop.gtk'),'owned GTK chooser backend',backend)
 portal=launch([tools['portal']],'chooser-portal')
 wait(lambda:connection.name_has_owner('org.freedesktop.portal.Desktop'),'owned standard chooser portal',portal)
 facts.update(backendPid=backend.pid,address=address,activation=False,systemBus=environment['DBUS_SYSTEM_BUS_ADDRESS'])
 connection.close()
 return facts

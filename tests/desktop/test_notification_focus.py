"""Run on an isolated D-Bus session; exercises actual delivery through Focus policy."""
import importlib.util,json,tempfile,time
from pathlib import Path
import dbus,dbus.mainloop.glib
source=Path(__file__).resolve().parents[2]/'res/lumen-desktop/notifications/service.py'
spec=importlib.util.spec_from_file_location('focus_delivery',source);s=importlib.util.module_from_spec(spec);spec.loader.exec_module(s)
s.read_settings=lambda:{'notifications':{'policies':{}}}
dbus.mainloop.glib.DBusGMainLoop(set_as_default=True)
with tempfile.TemporaryDirectory() as directory:
 server=s.Server(dbus.SessionBus(),Path(directory))
 def command(**data):server.Command(json.dumps(dict(action='focus',**data)))
 def notify(app,urgency=1):
  server.Notify(app,0,'','Example','Body',[],{'desktop-entry':app,'urgency':urgency},-1,sender=dbus.SessionBus().get_unique_name());return server.rows[0]
 command(operation='profile',profile=dict(id='work',name='Work',allowedApps=['calendar'],allowCritical=False,schedules=[]))
 command(profile='work')
 assert not notify('chat')['banner']
 assert notify('calendar')['banner']
 assert not notify('critical',2)['banner']
 assert len(server.rows)==3
 command(profile='')
 assert not any(row['banner'] for row in server.rows if row['app']!='calendar')
 assert notify('chat')['banner']
 command(profile='work',seconds=1)
 server.focus.until=time.time()-1;server.tick()
 assert not server.dnd and json.loads(server.state())['focus']['active']==''
 server.policies['calendar']='silent';command(profile='work')
 assert not notify('calendar')['banner']
 assert (Path(directory)/'focus.json').stat().st_mode&0o777==0o600
 s.GLib.source_remove(server.tick_source);server.bus.remove_signal_receiver(server.owner_changed,'NameOwnerChanged','org.freedesktop.DBus');server.remove_from_connection()
print('PASS actual delivery: Focus exceptions, blocked history, expiry, no replay and per-app silent precedence')

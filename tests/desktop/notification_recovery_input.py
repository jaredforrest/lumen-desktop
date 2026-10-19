"""Disposable notification producer, service, watcher and owned shell crash matrix."""
import argparse,json,os,re,select,shutil,signal,socket,subprocess as sp,sys,tempfile,time
from pathlib import Path
import dbus
p=argparse.ArgumentParser();p.add_argument('--sway',required=True);p.add_argument('--launcher',type=Path,required=True);p.add_argument('--effects',action='store_true');p.add_argument('--output',type=Path,required=True);a=p.parse_args();a.output.mkdir(parents=True,exist_ok=False)
processes=[]
with tempfile.TemporaryDirectory(prefix='nrecover-') as temporary:
 private=Path(temporary);prefix=re.split(r'\n[ \t]*exec quickshell ',a.launcher.read_text(),maxsplit=1)[0]
 env=dict(value.decode().split('=',1) for value in sp.check_output(['bash','-c',prefix+'\nexec env -0']).split(b'\0') if b'=' in value)
 for key in ('HOME','XDG_RUNTIME_DIR','XDG_CONFIG_HOME','XDG_STATE_HOME','XDG_DATA_HOME','XDG_CACHE_HOME'):
  env[key]=str(private/('r' if key=='XDG_RUNTIME_DIR' else key));Path(env[key]).mkdir(mode=0o700)
 for key in ('DISPLAY','WAYLAND_DISPLAY','SWAYSOCK','DBUS_SESSION_BUS_ADDRESS','AT_SPI_BUS_ADDRESS'):env.pop(key,None)
 env.update(WLR_BACKENDS='headless',WLR_RENDERER='gles2' if a.effects else 'pixman',WLR_RENDERER_FORCE_SOFTWARE='1' if a.effects else '0',WLR_RENDERER_ALLOW_SOFTWARE='1',LIBGL_ALWAYS_SOFTWARE='1',WLR_LIBINPUT_NO_DEVICES='1',QT_QPA_PLATFORM='wayland',QT_QUICK_BACKEND='software',QT_QPA_PLATFORMTHEME='generic',QT_NO_XDG_DESKTOP_PORTAL='1',QT_ACCESSIBILITY='0',NO_AT_BRIDGE='1',DBUS_SYSTEM_BUS_ADDRESS='unix:path='+str(private/'no-system-bus'),LUMEN_LAUNCH_SCOPES='0',LUMEN_LOCK_PREVIEWS='0')
 env['GLOBAL_MENU_PYTHON']=sys.executable;installed_assets=Path(env['LUMEN_NOTIFICATION_HELPER']).parent.parent;assert (installed_assets/'notifications/session_lock.py').is_file() and (installed_assets/'notifications/transport.py').is_file();env['LUMEN_NOTIFICATION_STATE']=str(private/'history');settings=private/'settings.json';env['LUMEN_NIX_SETTINGS_ACTIVE']=str(settings)
 def preference(value):settings.write_text(json.dumps({'notifications':{'preview':value,'policies':{}}}))
 preference('when_unlocked')
 def launch(cmd,name,**kw):
  proc=sp.Popen(cmd,env=env,stdout=(a.output/name).open('w'),stderr=sp.STDOUT,start_new_session=True,**kw);processes.append(proc);return proc
 def call(*cmd):return sp.check_output(cmd,env=env,text=True,timeout=5).strip()
 def wait(fn):
  deadline=time.monotonic()+15
  while time.monotonic()<deadline:
   try:
    value=fn()
    if value:return value
   except (ValueError,OSError,dbus.DBusException,sp.SubprocessError):pass
   time.sleep(.04)
  raise AssertionError('Notification recovery fixture timed out: '+str(a.output))
 try:
  bus_config=private/'bus.conf';bus_config.write_text('<busconfig><type>session</type><listen>unix:tmpdir='+env['XDG_RUNTIME_DIR']+'</listen><policy context="default"><allow own="*"/><allow send_destination="*"/><allow receive_sender="*"/></policy></busconfig>')
  bus=sp.Popen(['dbus-daemon','--nofork','--print-address=1','--config-file='+str(bus_config)],env=env,stdout=sp.PIPE,stderr=(a.output/'bus.log').open('w'),text=True,start_new_session=True);processes.append(bus);assert select.select([bus.stdout],[],[],5)[0];env['DBUS_SESSION_BUS_ADDRESS']=bus.stdout.readline().strip();connection=dbus.bus.BusConnection(env['DBUS_SESSION_BUS_ADDRESS'])
  config=private/'sway.conf';config.write_text('output HEADLESS-1 resolution 1100x800\nxwayland disable\nseat seat0 fallback true\n')
  env['SWAYSOCK']=str(Path(env['XDG_RUNTIME_DIR'])/'compositor.sock')
  def start_compositor(name):
   def live(path):
    probe=socket.socket(socket.AF_UNIX,socket.SOCK_STREAM)
    try:probe.settimeout(.1);probe.connect(str(path));return True
    except OSError:return False
    finally:probe.close()
   ipc_path=Path(env['SWAYSOCK'])
   if ipc_path.exists():assert not live(ipc_path);ipc_path.unlink()
   proc=launch([a.sway,'--unsupported-gpu','-c',str(config)],name);wait(lambda:live(ipc_path));env['WAYLAND_DISPLAY']=wait(lambda:next((f.name for f in Path(env['XDG_RUNTIME_DIR']).glob('wayland-*') if not f.name.endswith('.lock') and live(f)),None));return proc
  compositor=start_compositor('sway.log');server=launch([sys.executable,env['LUMEN_NOTIFICATION_HELPER'],'serve'],'notifications.log')
  def api(interface):return dbus.Interface(connection.get_object('org.freedesktop.Notifications','/org/freedesktop/Notifications'),interface)
  def snapshot():return json.loads(api('org.lumendesktop.Notifications').Snapshot())
  def crash(proc):
   os.killpg(proc.pid,signal.SIGKILL);proc.wait(timeout=5)
  wait(lambda:snapshot()['lock']=={'known':True,'locked':False})
  shutil.copytree(installed_assets,private/'assets')
  fixture=private/'fixture.qml';fixture.write_text('''import QtQuick
import QtQuick.Controls
import Quickshell
import Quickshell.Io
import "assets/shell" as D
ShellRoot {
 id:root;property int probes:0
 D.WidgetState {id:widgets}
 QtObject {id:stub;property var bars:({});function preferredOutput(){return "HEADLESS-1";}function claimPopup(owner){}}
 D.NotificationState {id:notifications;shell:stub}
 FloatingWindow {visible:true;title:"Recovery notification";implicitWidth:440;implicitHeight:300;color:D.Theme.colors.background
  Button {id:probe;x:20;y:250;text:"Recovery probe";onClicked:root.probes++}
  D.NotificationCard {id:card;x:20;y:20;width:400;visible:notifications.rows.length>0;service:notifications;notification:notifications.rows[0] || ({id:0,app:"",summary:"",body:"",icon:"",image:"",actions:[],timestamp:0})}
 }
 IpcHandler {target:"recoveryAudit"
  function status():string{return JSON.stringify({available:notifications.available,instance:notifications.instance,revision:notifications.revision,rows:notifications.rows,layout:widgets.preferences,probes:root.probes,lease:notifications.commandUntil});}
  function activate():void{card.activate();}
  function activateWithLease(value:string):void{const current=notifications.commandUntil;notifications.commandUntil=Number(value);card.activate();notifications.commandUntil=current;}
  function focusProbe():void{probe.forceActiveFocus();}
  function configure():void{widgets.addWidget("clock");const item=widgets.desktopItems[0];widgets.positionWidget(item.id,"HEADLESS-1",0.25,0.4);}
 }
}''')
  producer_script=private/'producer.py';producer_script.write_text('''import dbus,dbus.mainloop.glib,json,sys
from gi.repository import GLib
dbus.mainloop.glib.DBusGMainLoop(set_as_default=True)
bus=dbus.SessionBus();api=dbus.Interface(bus.get_object('org.freedesktop.Notifications','/org/freedesktop/Notifications'),'org.freedesktop.Notifications')
def action(ident,key):
 if int(ident)==int(own_id):print(json.dumps({'action':str(key),'id':int(ident)}),flush=True)
bus.add_signal_receiver(action,'ActionInvoked','org.freedesktop.Notifications')
own_id=api.Notify('Recovery producer',0,'',sys.argv[1],'Synthetic recovery body',['default','Open'],{},0)
print(json.dumps({'ready':int(own_id),'sender':bus.get_unique_name()}),flush=True)
GLib.MainLoop().run()
''')
  producer_index=0
  def producer(label):
   global producer_index
   producer_index+=1;name='producer-%d.log'%producer_index
   proc=launch([sys.executable,str(producer_script),label],name)
   wait(lambda:any('ready' in json.loads(line) for line in (a.output/name).read_text().splitlines()));return proc,a.output/name
  def signals(path):return [json.loads(line) for line in path.read_text().splitlines() if line.strip()]
  def actions(path):return [line for line in signals(path) if 'action' in line]
  def start_ui(name):return launch(['quickshell','-p',str(fixture)],name)
  def ui_state():return json.loads(call('quickshell','-p',str(fixture),'ipc','call','recoveryAudit','status'))
  def probe_control():
   before=ui_state()['probes'];call('quickshell','-p',str(fixture),'ipc','call','recoveryAudit','focusProbe');call('swaymsg','[title="Recovery notification"]','focus');call('wtype','-k','space');wait(lambda:ui_state()['probes']==before+1)
   assert ui_state()['layout']==saved_layout
  def activate():call('quickshell','-p',str(fixture),'ipc','call','recoveryAudit','activate')
  def child_watcher():
   found=[]
   for item in Path('/proc').glob('[0-9]*/cmdline'):
    try:
     command=item.read_bytes().split(b'\0')
     if str(env['LUMEN_NOTIFICATION_HELPER']).encode() in command and b'watch' in command:
      status=(item.parent/'status').read_text();ppid=int(next(line.split()[1] for line in status.splitlines() if line.startswith('PPid:')))
      if ppid==ui.pid:found.append(int(item.parent.name))
    except (OSError,ValueError,StopIteration):pass
   assert len(found)<=1,found
   return found[0] if found else None
  launch(['wtype','-s','60000'],'keyboard.log')
  first,first_log=producer('Before producer death');ui=start_ui('ui.log')
  wait(lambda:ui_state()['available'] and ui_state()['rows'] and ui_state()['rows'][0]['actions'])
  call('quickshell','-p',str(fixture),'ipc','call','recoveryAudit','configure')
  saved_layout=ui_state()['layout'];layout_file=Path(env['XDG_STATE_HOME'])/'lumen-desktop/session/notification-centre.json'
  wait(lambda:layout_file.exists() and json.loads(layout_file.read_text())==saved_layout)
  first_row=ui_state()['rows'][0];crash(first)
  wait(lambda:ui_state()['rows'] and not ui_state()['rows'][0]['actions'])
  activate();time.sleep(.2);assert not actions(first_log)
  checks=['Actual producer SIGKILL revokes actions without removing retained history']
  second,second_log=producer('Watcher restart action');wait(lambda:ui_state()['rows'][0]['summary']=='Watcher restart action')
  # A stopped watcher still owns its pipes: no process-exit signal can help UI.
  old_lease=ui_state()['lease'];old_watcher=wait(child_watcher);os.kill(old_watcher,signal.SIGSTOP)
  activate();time.sleep(.2);assert not actions(second_log)
  wait(lambda:not ui_state()['available'] and not ui_state()['rows']);probe_control()
  os.kill(old_watcher,signal.SIGCONT)
  wait(lambda:child_watcher() not in (None,old_watcher) and ui_state()['available'])
  assert not actions(second_log)
  call('quickshell','-p',str(fixture),'ipc','call','recoveryAudit','activateWithLease',str(old_lease))
  time.sleep(.2);assert not actions(second_log)
  wait(lambda:ui_state()['available'])
  checks.append('Delayed owned action retains its captured expired lease even after a fresh transport lease arrives')
  checks.append('Stopped transport becomes unavailable while other keyboard controls and layout stay usable; resume never replays its pending action')
  old_watcher=wait(child_watcher);os.kill(old_watcher,signal.SIGSTOP)
  activate();time.sleep(.2);assert not actions(second_log)
  os.kill(old_watcher,signal.SIGKILL)
  wait(lambda:not ui_state()['available'] and not ui_state()['rows'])
  wait(lambda:child_watcher() not in (None,old_watcher) and ui_state()['available'] and ui_state()['rows'][0]['summary']=='Watcher restart action')
  activate();wait(lambda:len(actions(second_log))==1);time.sleep(.25);assert len(actions(second_log))==1
  checks.append('Stopped transport with queued action is killed; reconnect drops stale command and fresh action dispatches once')
  third,third_log=producer('Service restart action');wait(lambda:ui_state()['rows'][0]['summary']=='Service restart action')
  os.kill(server.pid,signal.SIGSTOP);activate()
  wait(lambda:not ui_state()['available'] and not ui_state()['rows']);probe_control()
  os.kill(server.pid,signal.SIGCONT)
  wait(lambda:ui_state()['available'] and ui_state()['rows'][0]['summary']=='Service restart action')
  assert not actions(third_log)
  checks.append('Stopped server times out without freezing owned controls; queued expired action is rejected after resume and transport reconnects')
  old_instance=snapshot()['instance'];crash(server);wait(lambda:not ui_state()['available'] and not ui_state()['rows']);probe_control()
  server=launch([sys.executable,env['LUMEN_NOTIFICATION_HELPER'],'serve'],'notifications-restart.log')
  wait(lambda:snapshot()['instance']!=old_instance and snapshot()['lock']=={'known':True,'locked':False})
  wait(lambda:ui_state()['available'] and ui_state()['instance']==snapshot()['instance'])
  assert all(not row['actions'] and not row['active'] for row in ui_state()['rows'])
  assert any(row['summary']=='Service restart action' for row in ui_state()['rows'])
  activate();time.sleep(.2);assert not actions(third_log)
  checks.append('Service SIGKILL recovers retained history with fresh service identity and no revived producer action authority')
  fourth,fourth_log=producer('Shell restart action');wait(lambda:ui_state()['rows'][0]['summary']=='Shell restart action')
  retained=Path(env['LUMEN_NOTIFICATION_STATE'])/'history.json';history_before=retained.read_bytes()
  old_watcher=wait(child_watcher);os.kill(ui.pid,signal.SIGKILL);ui.wait(timeout=5)
  def watcher_gone():
   try:return '\nState:\tZ' in Path('/proc/'+str(old_watcher)+'/status').read_text()
   except FileNotFoundError:return True
  wait(watcher_gone);ui=start_ui('ui-restart.log')
  wait(lambda:ui_state()['available'] and ui_state()['rows'][0]['summary']=='Shell restart action')
  assert retained.read_bytes()==history_before and ui_state()['layout']==saved_layout
  probe_control()
  activate();wait(lambda:len(actions(fourth_log))==1);time.sleep(.25);assert len(actions(fourth_log))==1
  checks.append('Owned shell SIGKILL preserves widget layout, service history and producer identity; fresh keyboard control and action each dispatch once')
  for name in ('ui.log','ui-restart.log'):
   log=(a.output/name).read_text();assert not any(marker in log for marker in ('TypeError:','ReferenceError:','Binding loop','Failed to load configuration','Cannot assign')),log
  (a.output/'result.json').write_text(json.dumps({'passed':True,'checks':checks,'scope':'Private bus/compositor, synthetic producers and owned controls only; native widget drag/mask acceptance is separate widget_catalogue_input.py'},indent=2)+'\n')
  print('PASS disposable notification recovery matrix')
 finally:
  for proc in reversed(processes):
   if proc.poll() is None:
    try:os.killpg(proc.pid,signal.SIGTERM)
    except ProcessLookupError:pass
  for proc in reversed(processes):
   try:proc.wait(timeout=3)
   except sp.TimeoutExpired:os.killpg(proc.pid,signal.SIGKILL);proc.wait(timeout=3)

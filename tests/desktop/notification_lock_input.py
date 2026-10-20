"""Private native lock clients, D-Bus transport and painted notification privacy."""
from fixture_resources import close_reaped_pipes
from fixture_frames import latest_state_frame
import argparse,json,os,re,select,shutil,signal,socket,stat,subprocess as sp,sys,tempfile,time
from pathlib import Path
import dbus
p=argparse.ArgumentParser();p.add_argument('--sway',required=True);p.add_argument('--lock-client',required=True);p.add_argument('--launcher',type=Path,required=True);p.add_argument('--effects',action='store_true');p.add_argument('--output',type=Path,required=True);a=p.parse_args();a.output.mkdir(parents=True,exist_ok=False)
repo=Path(__file__).resolve().parents[2];processes=[]
with tempfile.TemporaryDirectory(prefix='nlock-') as temporary:
 private=Path(temporary);prefix=re.split(r'\n[ \t]*exec quickshell ',a.launcher.read_text(),maxsplit=1)[0]
 env=dict(value.decode().split('=',1) for value in sp.check_output(['bash','-c',prefix+'\nexec env -0']).split(b'\0') if b'=' in value)
 for key in ('HOME','XDG_RUNTIME_DIR','XDG_CONFIG_HOME','XDG_STATE_HOME','XDG_DATA_HOME','XDG_CACHE_HOME'):
  env[key]=str(private/('r' if key=='XDG_RUNTIME_DIR' else key));Path(env[key]).mkdir(mode=0o700)
 for key in ('DISPLAY','WAYLAND_DISPLAY','SWAYSOCK','DBUS_SESSION_BUS_ADDRESS','AT_SPI_BUS_ADDRESS'):env.pop(key,None)
 env.update(WLR_BACKENDS='headless',WLR_RENDERER='gles2' if a.effects else 'pixman',WLR_RENDERER_FORCE_SOFTWARE='1' if a.effects else '0',WLR_RENDERER_ALLOW_SOFTWARE='1',LIBGL_ALWAYS_SOFTWARE='1',WLR_LIBINPUT_NO_DEVICES='1',QT_QPA_PLATFORM='wayland',QT_QUICK_BACKEND='software',QT_QPA_PLATFORMTHEME='generic',QT_NO_XDG_DESKTOP_PORTAL='1',QT_ACCESSIBILITY='0',NO_AT_BRIDGE='1',DBUS_SYSTEM_BUS_ADDRESS='unix:path='+str(private/'no-system-bus'),LUMEN_LAUNCH_SCOPES='0',LUMEN_LOCK_PREVIEWS='0')
 env['GLOBAL_MENU_PYTHON']=sys.executable;installed_assets=Path(env['LUMEN_NOTIFICATION_HELPER']).parent.parent;assert (installed_assets/'notifications/session_lock.py').is_file() and (installed_assets/'notifications/transport.py').is_file();env['LUMEN_NOTIFICATION_STATE']=str(private/'history');settings=private/'settings.json';env['LUMEN_NIX_SETTINGS_ACTIVE']=str(settings)
 def preference(value):settings.write_text(json.dumps({'notifications':{'preview':value,'policies':{}}}))
 preference('when_unlocked')
 palette=json.loads((Path(__file__).resolve().parents[2]/'res/lumen-desktop/appearance/palette.json').read_text());appearance=private/'appearance.json';appearance.write_text(json.dumps(dict(font=palette['font'],palette=palette['dark'],preferences={})));env['LUMEN_APPEARANCE_STATE']=str(appearance)
 def launch(cmd,name,**kw):
  with (a.output/name).open('w') as _fixture_log0:
      proc=sp.Popen(cmd,env=env,stdout=_fixture_log0,stderr=sp.STDOUT,start_new_session=True,**kw);processes.append(proc);return proc
 def call(*cmd):return sp.check_output(cmd,env=env,text=True,timeout=5).strip()
 def wait(fn):
  deadline=time.monotonic()+15
  while time.monotonic()<deadline:
   try:
    value=fn()
    if value:return value
   except (ValueError,OSError,dbus.DBusException,sp.SubprocessError):pass
   time.sleep(.04)
  diagnostics={}
  for name,probe in [('ui',globals().get('ui_state')),('service',globals().get('snapshot')),('tree',lambda:json.loads(call(str(Path(a.sway).parent/'swaymsg'),'-t','get_tree','-r')))]:
   if not callable(probe):continue
   try:diagnostics[name]=probe()
   except (ValueError,OSError,dbus.DBusException,sp.SubprocessError) as error:diagnostics[name]={'error':str(error)}
  (a.output/'timeout-state.json').write_text(json.dumps(diagnostics,indent=2))
  raise AssertionError('Notification lock fixture timed out: '+str(a.output))
 def stop(proc):
  if proc.poll() is None:os.killpg(proc.pid,signal.SIGTERM)
  proc.wait(timeout=5)
 try:
  bus_config=private/'bus.conf';bus_config.write_text('<busconfig><type>session</type><listen>unix:tmpdir='+env['XDG_RUNTIME_DIR']+'</listen><policy context="default"><allow own="*"/><allow send_destination="*"/><allow receive_sender="*"/></policy></busconfig>')
  with (a.output/'bus.log').open('w') as _fixture_log0:
      bus=sp.Popen(['dbus-daemon','--nofork','--print-address=1','--config-file='+str(bus_config)],env=env,stdout=sp.PIPE,stderr=_fixture_log0,text=True,start_new_session=True);processes.append(bus);assert select.select([bus.stdout],[],[],5)[0];env['DBUS_SESSION_BUS_ADDRESS']=bus.stdout.readline().strip();connection=dbus.bus.BusConnection(env['DBUS_SESSION_BUS_ADDRESS'])
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
  icon=private/'Private-icon.png';shutil.copyfile(installed_assets/'shell/wallpaper.png',icon)
  def notify(label):
   image=dbus.Struct((dbus.Int32(1),dbus.Int32(1),dbus.Int32(4),dbus.Boolean(True),dbus.Int32(8),dbus.Int32(4),dbus.ByteArray([255,0,0,255])),signature='iiibiiay')
   return int(api('org.freedesktop.Notifications').Notify('Fixture',0,icon.as_uri(),'Private subject '+label,'Private body '+label,['default','Private action '+label],{'image-data':image},0))
  wait(lambda:snapshot()['lock']=={'known':True,'locked':False});first=notify('one');old_frame=snapshot();assert 'Private body one' in json.dumps(old_frame)
  shutil.copytree(installed_assets,private/'assets');fixture=private/'fixture.qml';fixture.write_text('''import QtQuick
import Quickshell
import Quickshell.Io
import "assets/shell" as D
ShellRoot {
 QtObject {id:stub;property var bars:({});function preferredOutput(){return "HEADLESS-1";}function claimPopup(owner){}}
 D.NotificationState {id:notifications;shell:stub}
 FloatingWindow {visible:true;implicitWidth:440;implicitHeight:300;color:D.Theme.colors.background
  D.NotificationCard {id:card;x:20;y:20;width:400;visible:notifications.rows.length>0;service:notifications;notification:notifications.rows[0] || ({id:0,app:"",summary:"",body:"",icon:"",image:"",actions:[],timestamp:0})}
 }
 IpcHandler {target:"notificationAudit"
  function inject(value:string):void{const frame=JSON.parse(value);if(frame.transport===undefined)frame.transport=String(notifications.transportEpoch);notifications.applyState(JSON.stringify(frame));}
  function status():string{return JSON.stringify({available:notifications.available,transport:notifications.transportEpoch,instance:notifications.instance,revision:notifications.revision,rows:notifications.rows,accessible:card.Accessible.name});}
 }
}''')
  palette=json.loads((installed_assets/'appearance/palette.json').read_text());appearance=Path(env['XDG_STATE_HOME'])/'lumen-desktop/appearance.json';appearance.parent.mkdir(parents=True,exist_ok=True);appearance.write_text(json.dumps({'font':palette['font'],'palette':palette['light'],'preferences':{}}))
  ui=launch(['quickshell','-p',str(fixture)],'ui.log')
  def ui_state():return json.loads(call('quickshell','-p',str(fixture),'ipc','call','notificationAudit','status'))
  wait(lambda:ui_state()['available'] and 'Private subject one' in ui_state()['accessible'])
  watcher=launch([sys.executable,env['LUMEN_NOTIFICATION_HELPER'],'watch'],'transport.log',stdin=sp.PIPE,text=True)
  def frames():
   result=[]
   for line in (a.output/'transport.log').read_text().splitlines():
    try:result.append(json.loads(line))
    except ValueError:pass
   return result
  def lock():
   proc=launch([a.lock_client],'protocol-lock-%d.log'%len(processes),stdin=sp.PIPE,text=True);wait(lambda:snapshot()['lock']=={'known':True,'locked':True});return proc
  def unlock(proc):proc.stdin.write('u\n');proc.stdin.flush();proc.wait(timeout=5);wait(lambda:snapshot()['lock']=={'known':True,'locked':False})
  def redacted():
   value=snapshot();assert not value['previewsVisible'];assert all(not row['image'] and not row['icon'] and not row['actions'] for row in value['rows']);assert 'Private subject' not in json.dumps(value) and 'Private body' not in json.dumps(value) and 'Private action' not in json.dumps(value);return value
  held=lock();redacted();wait(lambda:latest_state_frame(frames()).get('lock',{}).get('locked') is True);locked_position=next(index for index,frame in enumerate(frames()) if frame.get('lock')=={'known':True,'locked':True});wait(lambda:'Private subject' not in json.dumps(ui_state()));call('quickshell','-p',str(fixture),'ipc','call','notificationAudit','inject',json.dumps(old_frame));assert 'Private subject' not in json.dumps(ui_state());notify('two');redacted();wait(lambda:len(ui_state()['rows'])==2);assert 'Private body' not in json.dumps(ui_state());wait(lambda:len(latest_state_frame(frames()).get('rows',[]))==2);assert all('Private subject' not in json.dumps(frame) and 'Private body' not in json.dumps(frame) and 'Private action' not in json.dumps(frame) for frame in frames()[locked_position:])
  retained=Path(env['LUMEN_NOTIFICATION_STATE'])/'history.json';assert 'Private body one' in retained.read_text() and 'Private body two' in retained.read_text();assert stat.S_IMODE(retained.stat().st_mode)==0o600 and stat.S_IMODE(retained.parent.stat().st_mode)==0o700
  unlock(held);wait(lambda:'Private subject two' in ui_state()['accessible'])
  # The production lock view, with a test-only unlock endpoint in a private copy.
  auth=private/'assets/auth';auth.chmod(0o700);(auth/'lock.qml').chmod(0o600);(auth/'wallpaper.png').symlink_to(private/'assets/shell/wallpaper.png');auth_qml=auth/'lock.qml';text=auth_qml.read_text();head,tail=text.rsplit('}',1);auth_qml.write_text(head+'\n IpcHandler {target:"lockFixture";function unlock():void{sessionLock.locked=false;Qt.quit();}}\n}'+tail)
  locker=launch(['quickshell','-p',str(auth_qml)],'production-lock.log');wait(lambda:snapshot()['lock']['locked']);redacted();call('quickshell','-p',str(auth_qml),'ipc','call','lockFixture','unlock');locker.wait(timeout=5);wait(lambda:not snapshot()['lock']['locked'])
  # The fallback client is equally authoritative; client death does not unlock.
  fallback=launch(['swaylock','-c','080b0f'],'fallback-lock.log');wait(lambda:snapshot()['lock']['locked']);redacted();os.killpg(fallback.pid,signal.SIGKILL);fallback.wait(timeout=5);assert snapshot()['lock']['locked'];redacted();replacement=lock();unlock(replacement)
  preference('never');wait(lambda:snapshot()['preview']=='never');redacted();wait(lambda:'Private subject' not in json.dumps(ui_state()));call('grim',str(a.output/'redacted-notification.png'))
  preference('always');wait(lambda:snapshot()['previewsVisible']);held=lock();redacted()
  stop(server);wait(lambda:not ui_state()['available']);env['LUMEN_LOCK_PREVIEWS']='1';server=launch([sys.executable,env['LUMEN_NOTIFICATION_HELPER'],'serve'],'notifications-host-allowed.log');wait(lambda:snapshot()['previewsVisible'] and snapshot()['lock']['locked']);assert 'Private body' in json.dumps(snapshot())
  stop(server);wait(lambda:not ui_state()['available']);env['LUMEN_LOCK_PREVIEWS']='0';server=launch([sys.executable,env['LUMEN_NOTIFICATION_HELPER'],'serve'],'notifications-host-denied.log');wait(lambda:snapshot()['lock']['locked']);redacted();unlock(held)
  # Both compositor and notification transport restarts discard stale authority.
  old_instance=snapshot()['instance'];old_transport=ui_state()['transport'];stop(server);wait(lambda:not ui_state()['available'] and ui_state()['rows']==[]);server=launch([sys.executable,env['LUMEN_NOTIFICATION_HELPER'],'serve'],'notifications-restarted.log');wait(lambda:snapshot()['instance']!=old_instance and snapshot()['previewsVisible']);wait(lambda:ui_state()['instance']==snapshot()['instance'])
  current_ui=ui_state();stale=dict(old_frame,transport=str(old_transport),revision=current_ui['revision']+100);call('quickshell','-p',str(fixture),'ipc','call','notificationAudit','inject',json.dumps(stale));assert ui_state()['available'] and ui_state()['instance']==current_ui['instance'] and ui_state()['revision']==current_ui['revision'] and ui_state()['rows']==current_ui['rows']
  stop(compositor);assert compositor.returncode==0;wait(lambda:snapshot()['lock']=={'known':False,'locked':True});redacted();compositor=start_compositor('sway-reconnected.log');held=lock();redacted();unlock(held)
  stop(compositor);assert compositor.returncode==0
  (a.output/'result.json').write_text(json.dumps({'passed':True,'checks':['real native session-lock acquire and unlock','production lock view and fallback client','abandoned lock remains protected','subject body images and action transport redaction','protected original retained history','UI accessible label and painted redacted card','always constrained by host lock policy','never hides while unlocked','transport owner restart clears stale UI','native disconnect fails closed and fresh reconnect restores authority']},indent=2));print('PASS native notification lock privacy, transport and UI lifecycle')
 finally:
  for proc in reversed(processes):
   if proc.poll() is None:
    try:os.killpg(proc.pid,signal.SIGTERM)
    except ProcessLookupError:pass
  for proc in reversed(processes):
   try:proc.wait(timeout=3)
   except sp.TimeoutExpired:os.killpg(proc.pid,signal.SIGKILL);proc.wait(timeout=3)
  for _fixture_process in processes:
      if _fixture_process.poll() is None:_fixture_process.wait(timeout=5)
  close_reaped_pipes(processes)

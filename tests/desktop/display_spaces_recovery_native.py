"""Private two-output native cohort transitions; never operates a live session."""
import argparse,json,os,re,shutil,signal,subprocess as sp,sys,tempfile,time
from pathlib import Path
from application_fixture.isolation import run as isolated_run
from private_native_bus import start_private_bus
from fixture_resources import close_reaped_pipes
p=argparse.ArgumentParser();p.add_argument('--launcher',type=Path,required=True);p.add_argument('--sway',type=Path,required=True);p.add_argument('--output',type=Path);p.add_argument('--effects',action='store_true');p.add_argument('--inside',action='store_true');p.add_argument('--python-runtime',default=shutil.which('desktop-test-python'));p.add_argument('--egl-vendor',type=Path,default=os.environ.get('__EGL_VENDOR_LIBRARY_FILENAMES'));p.add_argument('--legacy',action='store_true');p.add_argument('--font-config',type=Path,required=True);p.add_argument('--source-revision');p.add_argument('--parent-net');a=p.parse_args()
if not a.inside:
 if not a.output or not a.python_runtime:p.error('Fresh artifacts and pinned test runtime required')
 a.output=a.output.resolve();a.output.mkdir(mode=0o700,parents=True,exist_ok=False)
 a.launcher=a.launcher.resolve();a.sway=a.sway.resolve();a.font_config=a.font_config.resolve();runtime=Path(a.python_runtime).resolve()
 if not all(str(path).startswith('/nix/store/') for path in (a.launcher,a.sway,a.font_config,runtime)):raise ValueError('Pinned native tools required')
 revision=sp.check_output(['git','rev-parse','HEAD'],cwd=Path(__file__).resolve().parents[2],text=True).strip()
 args=[str(runtime),'/source/tests/desktop/display_spaces_recovery_native.py','--inside','--launcher',str(a.launcher),'--sway',str(a.sway),'--font-config',str(a.font_config),'--source-revision',revision,'--parent-net',os.readlink('/proc/self/ns/net')]
 if a.legacy:args+=['--legacy']
 if a.effects:
  if not a.egl_vendor or not str(a.egl_vendor.resolve()).startswith('/nix/store/'):raise ValueError('Pinned Mesa vendor required')
  args+=['--effects','--egl-vendor',str(a.egl_vendor.resolve())]
 raise SystemExit(isolated_run(Path(__file__).resolve().parents[2],a.output,args,software_graphics=a.effects))
assert os.getuid()!=0 and os.readlink('/proc/self/ns/net')!=a.parent_net
assert not any((Path('/dev')/name).exists() for name in ('dri','snd','video0')) and Path('/dev/udmabuf').exists()==a.effects
a.output=Path('/artifacts')
repo=Path(__file__).resolve().parents[2];processes=[];original_socket=os.environ.get('SWAYSOCK')
with tempfile.TemporaryDirectory(prefix='lumen-display-spaces-') as temporary:
 private=Path(temporary);prefix=re.split(r'\n[ \t]*exec quickshell ',a.launcher.read_text(),maxsplit=1)[0]
 env=dict(v.decode().split('=',1) for v in sp.check_output(['bash','-c',prefix+'\nexec env -0']).split(b'\0') if b'=' in v)
 for key in ('HOME','XDG_RUNTIME_DIR','XDG_STATE_HOME','XDG_DATA_HOME','XDG_CONFIG_HOME','XDG_CACHE_HOME'):
  env[key]=str(private/key);Path(env[key]).mkdir(mode=0o700)
 for key in ('SWAYSOCK','WAYLAND_DISPLAY','DISPLAY','DBUS_SESSION_BUS_ADDRESS','AT_SPI_BUS_ADDRESS'):env.pop(key,None)
 env.update(WLR_BACKENDS='headless',WLR_HEADLESS_OUTPUTS='2',WLR_RENDERER='gles2' if a.effects else 'pixman',WLR_RENDERER_FORCE_SOFTWARE='1' if a.effects else '0',WLR_RENDERER_ALLOW_SOFTWARE='1',LIBGL_ALWAYS_SOFTWARE='1',WLR_LIBINPUT_NO_DEVICES='1',QT_QPA_PLATFORM='wayland',QT_QUICK_BACKEND='software',QT_QPA_PLATFORMTHEME='generic',QT_NO_XDG_DESKTOP_PORTAL='1',QT_ACCESSIBILITY='0',NO_AT_BRIDGE='1',GTK_A11Y='none')
 if a.effects:env['__EGL_VENDOR_LIBRARY_FILENAMES']=str(a.egl_vendor)
 env['FONTCONFIG_FILE']=str(a.font_config)
 settings=private/'settings.json';settings.write_text('{}');env['LUMEN_NIX_SETTINGS_ACTIVE']=str(settings)
 python=env.get('GLOBAL_MENU_PYTHON') or 'python3';backend=repo/'res/lumen-desktop/runtime/spaces.py'
 def launch(args,name,**kwargs):
  with (a.output/name).open('w') as log:proc=sp.Popen(args,env=env,stdout=log,stderr=sp.STDOUT,start_new_session=True,**kwargs)
  processes.append(proc);return proc
 def call(*args):return sp.check_output(list(map(str,args)),env=env,text=True,timeout=6).strip()
 def sway(*args):return json.loads(call(a.sway.parent/'swaymsg','-r',*args))
 def checked(command):
  result=sway(command);assert result and all(row.get('success') for row in result),(command,result)
 def tree():return sway('-t','get_tree')
 def visible():return {row['output']:row['name'] for row in sway('-t','get_workspaces') if row.get('visible')}
 def status():return json.loads(call(python,backend,'status'))
 def wait(fn):
  deadline=time.monotonic()+15
  while time.monotonic()<deadline:
   try:value=fn()
   except (sp.SubprocessError,OSError,ValueError):value=None
   if value:return value
   time.sleep(.04)
  (a.output/'timeout-tree.json').write_text(json.dumps(tree(),indent=2))
  (a.output/'timeout-seats.json').write_text(json.dumps(sway('-t','get_seats'),indent=2))
  raise AssertionError('Display-space observation deadline')
 def nodes(node):
  yield node
  for child in node.get('nodes',[])+node.get('floating_nodes',[]):yield from nodes(child)
 checks=[]
 try:
  start_private_bus(env,a.output,processes)
  config=private/'sway.conf';config.write_text('output HEADLESS-1 resolution 1000x700 position 0 0\noutput HEADLESS-2 resolution 900x700 position 1000 0\nseat seat0 fallback true\nxwayland disable\nfocus_follows_mouse no\n')
  launch([str(a.sway),'--unsupported-gpu','-c',str(config)],'sway.log')
  env['SWAYSOCK']=str(wait(lambda:next(Path(env['XDG_RUNTIME_DIR']).glob('sway-ipc*.sock'),None)))
  env['WAYLAND_DISPLAY']=wait(lambda:next((x.name for x in Path(env['XDG_RUNTIME_DIR']).glob('wayland-*') if not x.name.endswith('.lock')),None))
  launch(['wtype','-s','120000'],'keyboard.log')
  settings.write_text(json.dumps(dict(spaces=dict(separateDisplays=False))))
  if a.legacy:assert 'display_spaces' not in tree()
  else:assert status()['displaySpaces']['coupled']
  assets=private/'assets';shutil.copytree(repo/'res/lumen-desktop',assets)
  palette=json.loads((assets/'appearance/palette.json').read_text());appearance=private/'appearance.json';appearance.write_text(json.dumps(dict(font=palette['font'],palette=palette['dark'],preferences=dict(dockMagnification=False,reduceMotion=True))));env['LUMEN_APPEARANCE_STATE']=str(appearance)
  qml=private/'recovery.qml';qml.write_text('''import QtQuick
import QtQuick.Window
import Quickshell
import Quickshell.Io
import "assets/shell" as D
ShellRoot {id:stub
 D.DisplaySpacesState {id:policy}
 property alias displaySpaces:policy
 property string popupOwner:""
 property bool dockPreferencesBusy:false
 property var currentDate:new Date()
 property string aliasError:""
 property string applicationCommandError:""
 property var referenceDock:[{key:"synthetic",entry:{id:"synthetic",name:"Synthetic",icon:"utilities-terminal"},canvas:68,image:""}]
 signal requestDockFocus(string name)
 signal dismissPopups(string owner)
 function claimPopup(owner){popupOwner=owner;dismissPopups(owner);}
 function dockWindows(item){return [];}
 function dockLabel(item){return item.entry.name;}
 function activateDock(item){}
 function isDockPinned(entry){return true;}
 function canConfigureDockEntry(entry){return true;}
 function windowsFor(entry){return [];}
 function updateDockPin(entry,action){}
 function reorderDockPin(a,b){}
 Variants {id:docks;model:Quickshell.screens;D.Dock {required property var modelData;screen:modelData;shell:stub}}
 IpcHandler {target:"displaySpaceRecovery"
  function status():string{return JSON.stringify({policy:policy.state,accepted:policy.lastAccepted,routing:policy.routing,docks:docks.instances.map(d=>({name:d.outputName,visible:d.visible,keyboard:d.keyboardMode,active:d.contentItem.Window.window?d.contentItem.Window.window.active:false}))});}
  function focus():void{stub.requestDockFocus("HEADLESS-2");}
  function pause():void{policy.observer.signal(19);}
  function resume():void{policy.observer.signal(18);}
  function terminate():void{policy.observer.signal(15);}
 }
}''')
  launch(['quickshell','-p',str(qml)],'docks.log')
  def ipc(method):return call('quickshell','-p',qml,'ipc','call','displaySpaceRecovery',method)
  def ui():
   value=json.loads(ipc('status'));(a.output/'last-dock-state.json').write_text(json.dumps(value,indent=2));return value
  def visible_names():return sorted(row['name'] for row in ui()['docks'] if row['visible'])
  def keyboard(output):
   checked('bindsym Mod4+d exec quickshell -p '+str(qml)+' ipc call displaySpaceRecovery focus')
   call('wtype','-M','logo','-k','d','-m','logo');wait(lambda:any(row['name']==output and row['keyboard'] and row['active'] for row in ui()['docks']))
   call('wtype','-k','Escape');wait(lambda:not any(row['keyboard'] for row in ui()['docks']))
  if a.legacy:
   wait(lambda:ui()['accepted'] and ui()['accepted']['nativeSupported'] is False)
   assert not ui()['policy']['available'] and not ui()['policy']['applied'] and ui()['policy']['error'];assert visible_names()==['HEADLESS-1','HEADLESS-2']
   keyboard('HEADLESS-2');call('grim',str(a.output/'legacy-independent.png'));checks.append('Actual older compositor has no capability; explicit unapplied independent fallback keeps both production Docks keyboard-usable')
   ipc('terminate');wait(lambda:ui()['policy'].get('applied') is False and ui()['policy'].get('protocolVersion') is None);assert visible_names()==['HEADLESS-1','HEADLESS-2'];wait(lambda:ui()['policy'].get('nativeSupported') is False);checks.append('Legacy observer death/restart retains usable Docks without fabricating applied coupling')
  else:
   wait(lambda:ui()['policy'].get('applied') is True and visible_names()==['HEADLESS-1'])
   settings.write_text(json.dumps(dict(spaces=dict(separateDisplays=False,primaryDisplay=dict(kind='connector',name='HEADLESS-2')))));wait(lambda:ui()['policy'].get('applied') is False);assert visible_names()==['HEADLESS-1'];assert ui()['policy']['primaryDisplay']['name']=='HEADLESS-2';checks.append('Changed main-display preference remains unapplied and retains routing until native registration')
   assert status()['displaySpaces']['applied'];wait(lambda:ui()['policy'].get('applied') is True and visible_names()==['HEADLESS-2']);keyboard('HEADLESS-2')
   settings.write_text(json.dumps(dict(spaces=dict(separateDisplays=False))));assert status()['displaySpaces']['applied'];wait(lambda:ui()['policy'].get('applied') is True and visible_names()==['HEADLESS-1']);checks.append('Confirmed native main-display registration updates Dock routing; automatic remembered main restores independently of focus')
   ipc('pause');wait(lambda:ui()['policy'].get('applied') is False);assert visible_names()==['HEADLESS-1'];keyboard('HEADLESS-1');checks.append('Real SIGSTOP expires applied authority while retained main-Dock keyboard routing remains usable')
   checked('output HEADLESS-1 disable');wait(lambda:visible_names()==['HEADLESS-2']);keyboard('HEADLESS-2');call('grim',str(a.output/'paused-main-disconnected.png'));checks.append('Actual main-output loss during observer pause chooses a connected deterministic Dock fallback')
   checked('output HEADLESS-1 enable');wait(lambda:visible_names()==['HEADLESS-1']);assert status()['displaySpaces']['applied'];expected_generation=tree()['display_spaces']['generation'];ipc('resume');wait(lambda:ui()['policy'].get('applied') is True and ui()['policy'].get('generation')==expected_generation);checks.append('Observer resumes or restarts into newly captured native generation after topology recovery')
   ipc('terminate');wait(lambda:ui()['policy'].get('applied') is False);assert visible_names()==['HEADLESS-1'];wait(lambda:ui()['policy'].get('applied') is True);assert visible_names()==['HEADLESS-1'];checks.append('Actual observer process death/restart retains last validated routing and reacquires current native authority')
  (a.output/'result.json').write_text(json.dumps(dict(passed=True,checks=checks,source=a.source_revision,compositor=str(a.sway),legacy=a.legacy,liveSocketUnchanged=os.environ.get('SWAYSOCK')==original_socket),indent=2));print(json.dumps(dict(passed=True,checks=len(checks))))
 finally:
  for proc in reversed(processes):
   if proc.poll() is None:
    try:os.killpg(proc.pid,signal.SIGTERM)
    except ProcessLookupError:pass
  for proc in reversed(processes):
   try:proc.wait(timeout=5)
   except sp.TimeoutExpired:os.killpg(proc.pid,signal.SIGKILL);proc.wait(timeout=5)
  close_reaped_pipes(processes)

"""Exact owned main-window placement across real process and output lifetimes."""
import argparse,hashlib,json,os,re,shutil,signal,socket,subprocess as sp,sys,tempfile,time
from pathlib import Path
from application_fixture.isolation import run as isolated_run
from application_fixture.provenance import FrozenSource,generated
from fixture_resources import close_reaped_pipes
from private_native_bus import start_private_bus
from application_fixture.keyboard import PhysicalKeyboard
p=argparse.ArgumentParser();p.add_argument('--launcher',type=Path,required=True);p.add_argument('--sway',type=Path,required=True);p.add_argument('--output',type=Path,required=True);p.add_argument('--font-config',type=Path,required=True);p.add_argument('--pointer',type=Path,required=True);p.add_argument('--keyboard',type=Path,required=True);p.add_argument('--effects',action='store_true');p.add_argument('--egl-vendor',type=Path,default=os.environ.get('__EGL_VENDOR_LIBRARY_FILENAMES'));p.add_argument('--python-runtime',default=shutil.which('desktop-test-python'));p.add_argument('--legacy',action='store_true');p.add_argument('--inside',action='store_true');p.add_argument('--source-revision');p.add_argument('--parent-net');a=p.parse_args()
repo=Path(__file__).resolve().parents[2]
if not a.inside:
 a.output=a.output.resolve();a.output.mkdir(mode=0o700,parents=True,exist_ok=False)
 tools=[a.launcher.resolve(),a.sway.resolve(),a.font_config.resolve(),a.pointer.resolve(),Path(a.python_runtime).resolve(),a.keyboard.resolve()]
 if a.effects:tools.append(a.egl_vendor.resolve())
 frozen=FrozenSource(repo,__file__,tools)
 argv=[str(tools[4]),'/source/tests/desktop/owned_geometry_native.py','--inside','--launcher',str(tools[0]),'--sway',str(tools[1]),'--font-config',str(tools[2]),'--pointer',str(tools[3]),'--keyboard',str(tools[5]),'--output','/artifacts/result','--source-revision',frozen.revision,'--parent-net',os.readlink('/proc/self/ns/net')]
 if a.legacy:argv+=['--legacy']
 if a.effects:argv+=['--effects','--egl-vendor',str(tools[6])]
 code=1
 try:code=isolated_run(repo,a.output,argv,software_graphics=a.effects)
 finally:frozen.finish(a.output,code)
 raise SystemExit(code)
assert os.getuid()!=0 and os.readlink('/proc/self/ns/net')!=a.parent_net
assert [name for _,name in socket.if_nameindex()]==['lo']
assert not any((Path('/dev')/name).exists() for name in ('dri','snd','video0')) and Path('/dev/udmabuf').exists()==a.effects
out=a.output;out.mkdir(mode=0o700);processes=[];records=[];checks=[];original_socket=os.environ.get('SWAYSOCK')
with tempfile.TemporaryDirectory(prefix='owned-geometry-') as temporary:
 private=Path(temporary);env=dict(os.environ)
 for key in ('HOME','XDG_RUNTIME_DIR','XDG_STATE_HOME','XDG_CONFIG_HOME','XDG_DATA_HOME','XDG_CACHE_HOME'):
  env[key]=str(private/key);Path(env[key]).mkdir(mode=0o700)
 prefix=re.split(r'\n[ \t]*exec quickshell ',a.launcher.read_text(),maxsplit=1)[0]
 env.update(dict(value.decode().split('=',1) for value in sp.check_output(['bash','-c',prefix+'\nexec env -0'],env=env).split(b'\0') if b'=' in value))
 for key in ('DISPLAY','WAYLAND_DISPLAY','SWAYSOCK','DBUS_SESSION_BUS_ADDRESS','AT_SPI_BUS_ADDRESS'):env.pop(key,None)
 env.update(WLR_BACKENDS='headless',WLR_HEADLESS_OUTPUTS='2',WLR_RENDERER='gles2' if a.effects else 'pixman',WLR_RENDERER_FORCE_SOFTWARE='1' if a.effects else '0',WLR_RENDERER_ALLOW_SOFTWARE='1',LIBGL_ALWAYS_SOFTWARE='1',WLR_LIBINPUT_NO_DEVICES='1',QT_QPA_PLATFORM='wayland',QT_QPA_PLATFORMTHEME='generic',QT_QUICK_BACKEND='software',QT_ACCESSIBILITY='0',NO_AT_BRIDGE='1',GTK_A11Y='none',FONTCONFIG_FILE=str(a.font_config),LUMEN_NATIVE_WINDOW_ACTIONS='0',LUMEN_NATIVE_WINDOW_CONTROLS='1',LUMEN_UNIFIED_OWNED_HEADERS='0')
 if a.effects:env['__EGL_VENDOR_LIBRARY_FILENAMES']=str(a.egl_vendor)
 assets=private/'assets';shutil.copytree(repo/'res/lumen-desktop',assets)
 installed=Path(re.search(r'exec quickshell .*--path (\S+)/shell',a.launcher.read_text()).group(1))
 for name in ('runtime/owned_geometry.py','runtime/window_geometry.py','shell/WindowGeometry.qml','shell/SettingsWindow.qml','shell/CalendarWindow.qml','shell/WindowGeometryStatus.qml'):
  assert (installed/name).read_bytes()==(assets/name).read_bytes(),name
 palette=json.loads((assets/'appearance/palette.json').read_text());appearance=private/'appearance.json';env['LUMEN_APPEARANCE_STATE']=str(appearance)
 def preference(value):
  appearance.write_text(json.dumps(dict(font=palette['font'],palette=palette['dark'],preferences=dict(reduceMotion=True),windowsPreferences=dict(rememberOwnedGeometry=value))))
 preference(True)
 providers=private/'providers';providers.mkdir();provider=providers/'lumen-calendar';provider.write_text('#!'+sys.executable+'\nimport json,sys\nr=json.loads(sys.stdin.readline());assert r["action"] in ("list","load")\nprint(json.dumps(dict(configured=False,providers=[],events=[],days=[],total=0)))\n');provider.chmod(0o755);env['PATH']=str(providers)+':'+env['PATH']
 placement=Path(env['XDG_STATE_HOME'])/'lumen-desktop/owned-geometry/placements.json'
 def launch(argv,name,**kw):
  with (out/name).open('w') as log:proc=sp.Popen(list(map(str,argv)),env=env,stdout=log,stderr=sp.STDOUT,start_new_session=True,**kw)
  processes.append(proc);return proc
 def stop(proc):
  if proc.poll() is None:
   os.killpg(proc.pid,signal.SIGTERM)
   try:proc.wait(timeout=5)
   except sp.TimeoutExpired:os.killpg(proc.pid,signal.SIGKILL);proc.wait(timeout=5)
 def call(*args):return sp.check_output(list(map(str,args)),env=env,text=True,timeout=6).strip()
 def sway(*args):return json.loads(call(a.sway.parent/'swaymsg','-r',*args))
 def command(value):
  result=sway(value);assert result and all(row.get('success') for row in result),result
 def nodes(node):
  yield node
  for child in node.get('nodes',[])+node.get('floating_nodes',[]):yield from nodes(child)
 def native(cid):return next((n for n in nodes(sway('-t','get_tree')) if n.get('id')==cid),None)
 def wait(fn):
  end=time.monotonic()+15
  while time.monotonic()<end:
   try:value=fn()
   except (sp.SubprocessError,ValueError,OSError):value=None
   if value:return value
   time.sleep(.04)
  (out/'timeout-tree.json').write_text(json.dumps(sway('-t','get_tree'),indent=2));
  if 'state' in globals():
   try:(out/'timeout-state.json').write_text(json.dumps(state(),indent=2))
   except (OSError,ValueError,sp.SubprocessError):pass
  raise AssertionError('Owned geometry observation deadline')
 sequence=0
 def open_window(kind):
  global sequence
  sequence+=1;qml=private/('consumer-'+str(sequence)+'.qml')
  qml.write_text('''import QtQuick
import Quickshell
import Quickshell.Io
import "assets/shell" as D
ShellRoot {id:fixture
 property string kind:'''+json.dumps(kind)+'''
 D.ScreenSaverState {id:saver;active:false}
 QtObject {id:stub;property var screenSaverPreferences:saver;property string volume:"Volume: 0.5";property var audioState:({available:false,muted:false,outputs:[],inputs:[],streams:[],inputAvailable:false,sink:null,source:null});property var notifications:({dnd:false,available:false,applications:[],policies:{},history:[],error:""});property var search:({available:false,scanning:false,indexed:0});property var shortcuts:({available:false,bindings:[],error:""});property date currentDate:new Date(2026,9,20);function claimPopup(name){}function focusOwnedWindow(name){}function openSettings(name){}}
 QtObject {id:service
  property var preferences:D.Theme.preferences;property var windowsPreferences:D.Theme.windowsPreferences;property var menuBarPreferences:D.Theme.menuBarPreferences;property var desktopPreferences:D.Theme.desktopPreferences
  property var notificationsPreferences:({preview:"when_unlocked"});property var searchPreferences:({disabledProviders:[]});property var appearance:({});property var system:({system:"Disposable",desktop:"Disposable"})
  property bool available:true;property bool busy:false;property bool applying:false;property string error:"";property string warning:""
  function refresh(){}function setPreference(key,value){}function setWindowPreference(key,value){}function setMenuBarPreference(key,value){}function setDesktopPreference(key,value){}function setSearchPreference(key,value){}function setNotificationPreference(key,value){}
 }
 D.SettingsWindow {id:settings;shell:stub;service:service;page:"Appearance"}
 D.CalendarWindow {id:calendar;shell:stub}
 property var main:kind==="Settings"?settings:calendar
 property int staleCode:-1
 property string staleReply:""
 Process {id:staleWorker;stdout:StdioCollector {onStreamFinished:fixture.staleReply=text}onExited:(code,status)=>fixture.staleCode=code}
 Component.onCompleted:{calendar.calendarState.active=false;calendar.calendarState.loaded=true;main.visible=true;}
 function visit(item){if(item.ownerWindow===main && item.completedRequests!==undefined)return item;for(const child of item.children || []){const result=visit(child);if(result)return result;}return null;}
 function findNamed(item,name){if(item.objectName===name)return item;for(const child of item.children || []){const found=findNamed(child,name);if(found)return found;}return null;}
 function footerBounds(){const names=kind==="Settings"?["settings-navigation-viewport","settings-content-scroll"]:["calendar-sidebar-viewport","calendar-content-viewport"],footer=findNamed(main.contentItem,"owned-window-placement-footer");if(!footer)return null;const point=footer.mapToItem(main.contentItem,0,0);return {y:point.y,height:footer.height,viewports:names.map(name=>{const item=findNamed(main.contentItem,name),at=item.mapToItem(main.contentItem,0,0);return {name:name,bottom:at.y+item.height};})};}
 function visitStatus(item){if(item.objectName==="owned-window-placement-status")return item.visible && item.width>0 && item.height>0;for(const child of item.children || [])if(visitStatus(child))return true;return false;}
 IpcHandler {target:"placementFixture"
  function hideWindow():string{main.visible=false;return JSON.stringify({kind:fixture.kind,visible:main.visible});}
  function reopenWindow():string{if(fixture.kind==="Settings")settings.openPage("Appearance");else calendar.open();return JSON.stringify({kind:fixture.kind,visible:main.visible});}
  function stale(container:int):void{fixture.staleCode=-1;fixture.staleReply="";staleWorker.command=["lumen-window-geometry","--app","org.quickshell","--title",main.title,"--container",String(container)];staleWorker.running=true;}
  function status():string{const fit=visit(main.contentItem);return JSON.stringify({staleCode:fixture.staleCode,staleReply:fixture.staleReply,visible:main.visible,footer:fixture.footerBounds(),title:main.title,width:main.width,height:main.height,screen:main.screen?main.screen.name:"",fit:fit?{completed:fit.completedRequests,busy:fit.busy,error:fit.error,result:fit.result,persistenceStatus:fit.persistenceStatus,persistenceSupported:fit.persistenceSupported,statusShown:!!fixture.visitStatus(main.contentItem)}:null});}
 }
}''')
  proc=launch(['quickshell','-p',qml],kind+'-'+str(sequence)+'.log')
  def state():return json.loads(call('quickshell','-p',qml,'ipc','call','placementFixture','status'))
  current=wait(lambda:next((n for n in nodes(sway('-t','get_tree')) if n.get('pid')==proc.pid and n.get('name')==('System Settings' if kind=='Settings' else 'Calendar')),None))
  wait(lambda:(s:=state())['fit'] and s['fit']['completed']>0 and not s['fit']['busy'])
  assert not state()['fit']['error'],state()
  state.hide=lambda:call('quickshell','-p',qml,'ipc','call','placementFixture','hideWindow')
  state.show=lambda:call('quickshell','-p',qml,'ipc','call','placementFixture','reopenWindow')
  def stale(container):return call('quickshell','-p',qml,'ipc','call','placementFixture','stale',str(container))
  return proc,current['id'],state,stale
 def saved(key):return json.loads(placement.read_text())['owners'].get(key) if placement.exists() else None
 try:
  start_private_bus(env,out,processes)
  config=private/'sway.conf';config.write_text('output HEADLESS-1 resolution 1280x900 position 0 0\noutput HEADLESS-2 resolution 1000x800 position 1280 0\nseat seat0 fallback true\nxwayland disable\nfocus_follows_mouse no\nfloating_modifier Mod4 normal\nfor_window [all] floating enable, border normal 1\n')
  launch([a.sway,'--unsupported-gpu','-c',config],'sway.log');env['SWAYSOCK']=str(wait(lambda:next(Path(env['XDG_RUNTIME_DIR']).glob('sway-ipc*.sock'),None)));env['WAYLAND_DISPLAY']=wait(lambda:next((p.name for p in Path(env['XDG_RUNTIME_DIR']).glob('wayland-*') if not p.name.endswith('.lock')),None))
  launch([a.sway.parent/'swaymsg','-m','-t','subscribe','["window","output"]'],'native-events.jsonl')
  command('lumen_window margins disable');command('focus output HEADLESS-1')
  pointer=launch([a.pointer,'pointer','move','0','0'],'pointer.log',stdin=sp.PIPE,text=True);pointer.stdin.write('m 0 0\n');pointer.stdin.flush();wait(lambda:any(i.get('type')=='pointer' for i in sway('-t','get_inputs')))
  if a.legacy:
   for kind in ('Settings','Calendar'):
    proc,cid,state,stale=open_window(kind)
    assert state()['fit']['persistenceSupported'] is False and state()['fit']['persistenceStatus']=='Remembered placement requires the updated compositor at next login' and state()['fit']['statusShown'] and not placement.exists(),state()
    bounds=state()['footer'];assert bounds['height']>0 and all(view['bottom']<=bounds['y'] for view in bounds['viewports']),bounds
    call('grim',out/(kind.lower()+'-unavailable.png'))
    checks.append(kind+' old native capability visibly refuses opted-in persistence while normal fit remains usable')
    preference(False);wait(lambda:state()['fit']['persistenceStatus']=='')
    count=state()['fit']['completed'];command('[con_id=%d] move container to output HEADLESS-2'%cid)
    wait(lambda:state()['fit']['completed']>count and not state()['fit']['busy']);assert not state()['fit']['error'] and not placement.exists()
    checks.append(kind+' opt-out keeps legacy native normal workarea fitting functional without records')
    stop(proc);wait(lambda:native(cid) is None);preference(True);command('focus output HEADLESS-1')
   (out/'result.json').write_text(json.dumps(dict(passed=True,checks=checks,source=a.source_revision,legacy=True,compositor=str(a.sway),launcher=str(a.launcher)),indent=2))
   raise SystemExit(0)
  keyboard_log=(out/'keyboard.log').open('w');keyboard=PhysicalKeyboard(a.keyboard,env,keyboard_log);processes.append(keyboard.process)
  for kind,key in (('Settings','settings.main'),('Calendar','calendar.main')):
   proc,cid,state,stale=open_window(kind);first_pid=proc.pid
   completed=state()['fit']['completed'];command('[con_id=%d] move container to output HEADLESS-2'%cid);wait(lambda:state()['screen']=='HEADLESS-2' and state()['fit']['completed']>completed and not state()['fit']['busy'])
   command('[con_id=%d] resize set width 650 px height 433 px, move absolute position 1400 px 110 px'%cid)
   node=native(cid);r=node['rect'];command('seat seat0 cursor set %d %d'%(r['x']+200,r['y']-node['deco_rect']['height']+16))
   pointer.stdin.write('m 0 0\np\nm 60 40\nr\n');pointer.stdin.flush()
   initial=wait(lambda:(s:=saved(key)) and s['width']==648 and s['height']==400 and s['output']=='HEADLESS-2' and s['x']==180 and s['y']==150 and s)
   hidden=cid;state.hide();wait(lambda:native(hidden) is None);completed=state()['fit']['completed'];reply=json.loads(state.show());assert reply['visible'] is True,reply
   cid=wait(lambda:next((n['id'] for n in nodes(sway('-t','get_tree')) if n.get('pid')==proc.pid and n.get('name')==state()['title']),None))
   wait(lambda:state()['fit']['completed']>completed and not state()['fit']['busy'])
   node=native(cid);assert node['rect']['x']==1460 and node['rect']['y']-node['deco_rect']['height']==150,(node,state())
   checks.append(kind+' same-process public hide/show preserves last committed geometry across actual native mapping')
   stop(proc);wait(lambda:native(cid) is None)
   proc,newid,state,stale=open_window(kind);assert proc.pid!=first_pid and newid!=cid
   wait(lambda:state()['screen']=='HEADLESS-2' and state()['width']==initial['width'] and state()['height']==initial['height'])
   before=placement.read_bytes();unchanged=native(newid);stale(cid);wait(lambda:state()['staleCode']==1);assert 'unavailable' in state()['staleReply'] and native(newid)['rect']==unchanged['rect'] and placement.read_bytes()==before
   checks.append(kind+' retired native container from previous process cannot retarget new singleton')
   node=native(newid);assert node['rect']['x']==1460 and node['rect']['y']-node['deco_rect']['height']==initial['y'],node
   call('grim',out/(kind.lower()+'-restored.png'))
   records.append(dict(kind=kind,firstPid=first_pid,newPid=proc.pid,retiredContainer=cid,restoredContainer=newid,record=initial,restored=node));checks.append(kind+' real new-process reopen restores exact registered client size/output-relative position')
   command('output HEADLESS-2 resolution 700x520');wait(lambda:state()['width']<=668 and state()['height']<=(337 if kind=='Settings' else 387) and not state()['fit']['busy']);assert placement.read_bytes()==before
   checks.append(kind+' changed output clamps reachable size without overwriting preferred placement')
   stop(proc);wait(lambda:native(newid) is None);command('output HEADLESS-2 disable');command('focus output HEADLESS-1')
   proc,newid,state,stale=open_window(kind);wait(lambda:state()['screen']=='HEADLESS-1');assert placement.read_bytes()==before
   node=native(newid);assert 0<=node['rect']['x'] and node['rect']['x']+node['rect']['width']<=1280
   checks.append(kind+' missing output reopens reachable on existing workspace and retains preferred output record')
   command('[con_id=%d] resize set width 622 px height 393 px, move absolute position 100 px 140 px'%newid)
   node=native(newid);r=node['rect'];command('seat seat0 cursor set %d %d'%(r['x']+200,r['y']-node['deco_rect']['height']+16))
   pointer.stdin.write('m 0 0\np\nm 20 10\nr\n');pointer.stdin.flush()
   wait(lambda:(s:=saved(key)) and s['width']==620 and s['height']==360 and s['output']=='HEADLESS-1');checks.append(kind+' subsequent ordinary native move/resize updates durable placement')
   node=native(newid);r=node['rect'];command('[con_id=%d] focus'%newid);wait(lambda:native(newid).get('focused'))
   command('seat seat0 cursor set %d %d'%(r['x']+r['width']*3//4,r['y']+r['height']*3//4));keyboard.send('m',64)
   pointer.stdin.write('m 0 0\nb 273 1\nm 30 20\nb 273 0\n');pointer.stdin.flush()
   wait(lambda:state()['width']!=620 or state()['height']!=360);keyboard.send('m',0)
   wait(lambda:(s:=saved(key)) and s['width']==state()['width'] and s['height']==state()['height'])
   checks.append(kind+' real modifier-pointer floating resize is observed and retained without a restore feedback loop')
   last=saved(key);hidden=newid;state.hide();wait(lambda:native(hidden) is None);completed=state()['fit']['completed'];reply=json.loads(state.show());assert reply['visible'] is True,reply
   newid=wait(lambda:next((n['id'] for n in nodes(sway('-t','get_tree')) if n.get('pid')==proc.pid and n.get('name')==state()['title']),None))
   wait(lambda:state()['fit']['completed']>completed and not state()['fit']['busy'])
   node=native(newid);assert state()['width']==last['width'] and state()['height']==last['height'] and node['rect']['x']==last['x'] and node['rect']['y']-node['deco_rect']['height']==last['y'],(last,node,state())
   checks.append(kind+' later in-session resized placement survives same-process public hide/show without older-record replay')
   preference(False);before=placement.read_bytes();command('[con_id=%d] resize set width 642 px height 413 px'%newid);wait(lambda:state()['width']==640);assert placement.read_bytes()==before
   stop(proc);wait(lambda:native(newid) is None);proc,optid,state,stale=open_window(kind);assert state()['width']!=json.loads(before)['owners'][key]['width'] and placement.read_bytes()==before
   checks.append(kind+' opt-out refuses capture and new-process restore while retaining inert records')
   stop(proc);wait(lambda:native(optid) is None);preference(True);command('output HEADLESS-2 enable');command('output HEADLESS-2 resolution 1000x800');command('focus output HEADLESS-1')
  # Unsupported owner keys fail before resolving or manipulating any live native window.
  before=placement.read_bytes();failure=sp.run(['lumen-window-geometry','--watch','--owner-key','files.1','--app','org.quickshell','--title','System Settings'],env=env,capture_output=True,text=True,timeout=6)
  assert failure.returncode==1 and 'Unsupported owned geometry registration' in failure.stdout and placement.read_bytes()==before
  checks.append('Unsupported registration refuses without changing records or dispatching a native action')
  call('grim',out/'reachable-desktop.png')
  (out/'result.json').write_text(json.dumps(dict(passed=True,checks=checks,records=records,source=a.source_revision,launcher=str(a.launcher),compositor=str(a.sway),effects=a.effects,liveSocketUnchanged=os.environ.get('SWAYSOCK')==original_socket,installedSourceMatched=True),indent=2))
 finally:
  (out/'completed-checks.json').write_text(json.dumps(dict(checks=checks,records=records),indent=2))
  if placement.exists():shutil.copyfile(placement,out/'placements.json')
  generated(private,out)
  for proc in reversed(processes):stop(proc)
  close_reaped_pipes(processes)
  if 'keyboard_log' in globals():keyboard_log.close()
  warnings={}
  for log in out.glob('*.log'):
   lines=[line for line in log.read_text(errors='replace').splitlines() if re.search(r'\b(?:WARN|ERROR)\b|ResourceWarning|Traceback',line)]
   if lines:warnings[log.name]=lines
  (out/'warning-scan.json').write_text(json.dumps(warnings,indent=2))
  assert not warnings,warnings
print(json.dumps(dict(passed=True,checks=len(checks))))

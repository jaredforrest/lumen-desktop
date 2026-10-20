"""Private native minimized-window destination with exact retained handles."""
import argparse,json,os,re,shutil,signal,subprocess as sp,sys,tempfile,time
from pathlib import Path
from application_fixture.isolation import run as isolated_run
from application_fixture.provenance import FrozenSource,generated
from private_native_bus import start_private_bus
from fixture_resources import close_reaped_pipes
p=argparse.ArgumentParser();p.add_argument('--launcher',type=Path,required=True);p.add_argument('--sway',type=Path,required=True);p.add_argument('--output',type=Path);p.add_argument('--effects',action='store_true');p.add_argument('--inside',action='store_true');p.add_argument('--python-runtime',default=shutil.which('desktop-test-python'));p.add_argument('--egl-vendor',type=Path,default=os.environ.get('__EGL_VENDOR_LIBRARY_FILENAMES'));p.add_argument('--pointer',type=Path,required=True);p.add_argument('--font-config',type=Path,required=True);p.add_argument('--source-revision');p.add_argument('--parent-net');a=p.parse_args()
if not a.inside:
 if not a.output or not a.python_runtime:p.error('Fresh artifacts and pinned test runtime required')
 a.output=a.output.resolve();a.output.mkdir(mode=0o700,parents=True,exist_ok=False)
 a.launcher=a.launcher.resolve();a.sway=a.sway.resolve();a.pointer=a.pointer.resolve();a.font_config=a.font_config.resolve();runtime=Path(a.python_runtime).resolve()
 if not all(str(path).startswith('/nix/store/') for path in (a.launcher,a.sway,a.pointer,a.font_config,runtime)):raise ValueError('Pinned native tools required')
 frozen=FrozenSource(Path(__file__).resolve().parents[2],__file__,[a.launcher,a.sway,a.pointer,a.font_config,runtime]+([a.egl_vendor] if a.effects else []));revision=frozen.revision
 args=[str(runtime),'/source/tests/desktop/minimized_dock_native.py','--inside','--launcher',str(a.launcher),'--sway',str(a.sway),'--pointer',str(a.pointer),'--font-config',str(a.font_config),'--source-revision',revision,'--parent-net',os.readlink('/proc/self/ns/net')]
 if a.effects:
  if not a.egl_vendor or not str(a.egl_vendor.resolve()).startswith('/nix/store/'):raise ValueError('Pinned Mesa vendor required')
  args+=['--effects','--egl-vendor',str(a.egl_vendor.resolve())]
 code=1
 try:code=isolated_run(Path(__file__).resolve().parents[2],a.output,args,software_graphics=a.effects)
 finally:frozen.finish(a.output,code)
 raise SystemExit(code)
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
  if 'ui' in globals():
   (a.output/'timeout-ui.json').write_text(json.dumps(ui(),indent=2));call('grim',a.output/'timeout.png')
  (a.output/'timeout-tree.json').write_text(json.dumps(tree(),indent=2))
  (a.output/'timeout-seats.json').write_text(json.dumps(sway('-t','get_seats'),indent=2))
  raise AssertionError('Window-control observation deadline')
 def nodes(node):
  yield node
  for child in node.get('nodes',[])+node.get('floating_nodes',[]):yield from nodes(child)


 def native(cid):return next((n for n in nodes(tree()) if n.get('id')==cid),None)
 def make(key):
  qml=private/(key+'.qml');qml.write_text('import QtQuick\nimport Quickshell\nShellRoot {FloatingWindow {visible:true;title:"Same document title";implicitWidth:400;implicitHeight:240;TextEdit {anchors.fill:parent;focus:true;text:'+json.dumps(key)+'}}}')
  existing={n['id'] for n in nodes(tree())};proc=launch(['quickshell','-p',str(qml)],key+'.log');node=wait(lambda:next((n for n in nodes(tree()) if n.get('name')=='Same document title' and n['id'] not in existing),None));return proc,node['id']
 def point(index):
  value=ui();x=(1280-value['width'])/2+12+(index+.5)*value['slot'];y=900-7-value['shelf']/2
  checked('seat seat0 cursor set %d %d'%(x,y));pointer.stdin.write('m 0 0\n');pointer.stdin.flush();wait(lambda:ui()['hovered']==index)
 def button(value):pointer.stdin.write(value+'\n');pointer.stdin.flush()
 checks=[]
 try:
  start_private_bus(env,a.output,processes)
  config=private/'sway.conf';config.write_text('output HEADLESS-1 resolution 1280x900 position 0 0\noutput HEADLESS-2 resolution 1024x768 position 1280 0\nseat seat0 fallback true\nxwayland disable\nfocus_follows_mouse no\nfor_window [all] floating enable, border normal 1\n')
  launch([str(a.sway),'--unsupported-gpu','-c',str(config)],'sway.log');env['SWAYSOCK']=str(wait(lambda:next(Path(env['XDG_RUNTIME_DIR']).glob('sway-ipc*.sock'),None)));env['WAYLAND_DISPLAY']=wait(lambda:next((x.name for x in Path(env['XDG_RUNTIME_DIR']).glob('wayland-*') if not x.name.endswith('.lock')),None))
  pointer=launch([str(a.pointer),'pointer','move','0','0'],'pointer.log',stdin=sp.PIPE,text=True);pointer.stdin.write('m 0 0\n');pointer.stdin.flush();launch(['wtype','-s','120000'],'keyboard.log');wait(lambda:any(s.get('capabilities',0)&1 for s in sway('-t','get_seats')))
  assets=private/'assets';shutil.copytree(repo/'res/lumen-desktop',assets);palette=json.loads((assets/'appearance/palette.json').read_text());appearance=private/'appearance.json';appearance.write_text(json.dumps(dict(font=palette['font'],palette=palette['dark'],preferences=dict(dockMagnification=False,reduceMotion=True,dockMinimizeDestination='application'))));env['LUMEN_APPEARANCE_STATE']=str(appearance)
  qml=private/'dock.qml';qml.write_text('''import QtQuick
import Quickshell
import Quickshell.Io
import Quickshell.Wayland
import "assets/shell" as D
import "assets/shell/DockModel.js" as Model
ShellRoot {id:stub
 property var entry:({id:"synthetic-documents",name:"Documents",icon:"application-x-executable"})
 property var referenceDock:Model.build([], [entry],[entry],[],[],false,[],[],mini.items)
 property string popupOwner:""
 property bool dockPreferencesBusy:false
 property string aliasError:""
 property string applicationCommandError:""
 property date currentDate:new Date()
 property var windows:ToplevelManager.toplevels.values.filter(w=>w.title==="Same document title")
 property var captured:null
 property int attempts:0
 property int launches:0
 signal requestDockFocus(string name)
 signal dismissPopups(string owner)
 function claimPopup(owner){popupOwner=owner;dismissPopups(owner);}
 function windowsFor(e){return windows;}
 function dockWindows(item){return item.action==="minimized-window"?(mini.valid(item)?[item.window]:[]):windows;}
 function dockLabel(item){return item.action==="minimized-window"?item.name:entry.name;}
 function runningFor(e){return windows.length>0;}
 function canConfigureDockEntry(e){return false;}
 function isDockPinned(e){return false;}
 function canForceQuit(e){return false;}
 function activateDock(item,fresh,output){if(item.action!=="minimized-window"){launches++;return;}attempts++;mini.activate(item);}
 function activate(e,fresh){launches++;}
 function focusWindow(window){window.activate();}
 function updateDockPin(e,a){}
 function reorderDockPin(a,b){}
 D.MinimizedWindowState {id:mini;enabled:D.Theme.preferences.dockMinimizeDestination==="windows";windows:stub.windows;entryForWindow:w=>stub.entry}
 D.Dock {id:dock;screen:Quickshell.screens.find(s=>s.name==="HEADLESS-1");shell:stub}
 IpcHandler {target:"minimizedDock"
 function status():string{return JSON.stringify({output:dock.outputName,mode:D.Theme.preferences.dockMinimizeDestination,keyboard:dock.keyboardMode,keyboardIndex:dock.keyboardIndex,active:dock.contentItem.Window.window?dock.contentItem.Window.window.active:false,width:dock.width,shelf:dock.shelfThickness,slot:dock.slot,hovered:dock.hovered,pressed:dock.pressedKey,keys:stub.referenceDock.map(i=>i.key),entries:mini.items.map(i=>({key:i.key,name:i.name})),records:mini.records.map(r=>r.key),minimized:stub.windows.map(w=>w.minimized),attempts:stub.attempts,launches:stub.launches});}
 function mode(value:string):void{D.Theme.preferences=Object.assign({},D.Theme.preferences,{dockMinimizeDestination:value});}
 function focus():void{stub.requestDockFocus("HEADLESS-1");}
 function capture(key:string):void{stub.captured=mini.items.find(i=>i.key===key);}
 function stale():bool{return mini.activate(stub.captured);}
 }
}''')
  launch(['quickshell','-p',str(qml)],'dock.log')
  def ipc(action,*args):return call('quickshell','-p',qml,'ipc','call','minimizedDock',action,*args)
  def ui():return json.loads(ipc('status'))
  wait(lambda:ui()['mode']=='application');checked('focus output HEADLESS-1');first,cid1=make('first');wait(lambda:len(ui()['records'])==1);key1=ui()['records'][0]
  checked('focus output HEADLESS-2');second,cid2=make('second');wait(lambda:len(ui()['records'])==2);key2=next(k for k in ui()['records'] if k!=key1)
  boxes={cid:dict(native(cid)['rect']) for cid in (cid1,cid2)}
  for cid in (cid1,cid2):checked('[con_id=%d] lumen_window minimize'%cid)
  wait(lambda:all(ui()['minimized']));assert ui()['keys']==['synthetic-documents'];checks.append('Default application destination retains one app item for two same-title minimized documents')
  ipc('mode','windows');wait(lambda:len(ui()['entries'])==2);assert {r['key'] for r in ui()['entries']}=={key1,key2};checks.append('Separate destination publishes distinct ephemeral handles despite identical titles')
  ipc('focus');wait(lambda:ui()['keyboard'] and ui()['active']);index=ui()['keys'].index(key2);call('wtype','-k','Home',*sum((['-k','Right'] for _ in range(index)),[]));wait(lambda:ui()['keyboardIndex']==index);call('wtype','-k','Return');wait(lambda:not native(cid2)['lumen_state']['minimised'] and native(cid2)['focused']);assert native(cid1)['lumen_state']['minimised'] and native(cid2)['rect']==boxes[cid2] and native(cid2)['visible'],dict(first=native(cid1),second=native(cid2),boxes=boxes);checks.append('Physical keyboard activates exact second handle, clears native minimized state and restores original output/geometry')
  checked('[con_id=%d] lumen_window minimize'%cid2);wait(lambda:len(ui()['entries'])==2);point(ui()['keys'].index(key1));button('p');button('r');wait(lambda:not native(cid1)['lumen_state']['minimised'] and native(cid1)['focused']);assert native(cid2)['lumen_state']['minimised'] and native(cid1)['rect']==boxes[cid1] and native(cid1)['visible'],dict(first=native(cid1),second=native(cid2),boxes=boxes);checks.append('Physical pointer independently restores first handle without restoring its same-title sibling')
  checked('[con_id=%d] lumen_window minimize'%cid1);wait(lambda:len(ui()['entries'])==2);ipc('capture',key1);point(ui()['keys'].index(key1));button('p');wait(lambda:ui()['pressed']==key1);checked('[con_id=%d] kill'%cid1);wait(lambda:native(cid1) is None);third,cid3=make('replacement');checked('[con_id=%d] lumen_window minimize'%cid3);wait(lambda:len(ui()['entries'])==2);before=ui()['attempts'];button('r');wait(lambda:ui()['pressed']=='');assert ipc('stale')=='false' and native(cid3)['lumen_state']['minimised'] and native(cid2)['lumen_state']['minimised'] and ui()['attempts']==before and ui()['launches']==0;checks.append('Held pointer and retired capability cannot restore or launch a same-title replacement')
  key3=next(r['key'] for r in ui()['entries'] if r['key']!=key2);point(ui()['keys'].index(key3));button('p');wait(lambda:ui()['pressed']==key3);ipc('mode','application');wait(lambda:len(ui()['entries'])==0);button('r');wait(lambda:ui()['pressed']=='');assert native(cid2)['lumen_state']['minimised'] and native(cid3)['lumen_state']['minimised'] and ui()['launches']==0;checks.append('Destination change retires held item without app relaunch or bystander restore')
  call('grim',a.output/'application-destination.png');ipc('mode','windows');wait(lambda:len(ui()['entries'])==2);call('grim',a.output/'window-destination.png')
  (a.output/'result.json').write_text(json.dumps(dict(passed=True,checks=checks,source=a.source_revision,compositor=str(a.sway),nativeIds=[cid1,cid2,cid3],liveSocketUnchanged=os.environ.get('SWAYSOCK')==original_socket),indent=2));print(json.dumps(dict(passed=True,checks=len(checks))))
 finally:
  generated(private,a.output)
  for proc in reversed(processes):
   if proc.poll() is None:
    try:os.killpg(proc.pid,signal.SIGTERM)
    except ProcessLookupError:pass
  for proc in reversed(processes):
   try:proc.wait(timeout=5)
   except sp.TimeoutExpired:os.killpg(proc.pid,signal.SIGKILL);proc.wait(timeout=5)
  close_reaped_pipes(processes)

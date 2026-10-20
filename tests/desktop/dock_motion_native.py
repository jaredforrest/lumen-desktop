"""Private physical Dock motion and captured reorder reconstruction."""
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
 args=[str(runtime),'/source/tests/desktop/dock_motion_native.py','--inside','--launcher',str(a.launcher),'--sway',str(a.sway),'--pointer',str(a.pointer),'--font-config',str(a.font_config),'--source-revision',revision,'--parent-net',os.readlink('/proc/self/ns/net')]
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
with tempfile.TemporaryDirectory(prefix='lumen-dock-motion-') as temporary:
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
  raise AssertionError('Dock motion observation deadline')
 def nodes(node):
  yield node
  for child in node.get('nodes',[])+node.get('floating_nodes',[]):yield from nodes(child)



 def ui():return json.loads(call('quickshell','-p',qml,'ipc','call','dockAudit','status'))
 def ipc(action,*args):return call('quickshell','-p',qml,'ipc','call','dockAudit',action,*args)
 def write_count():return len(writes.read_text().splitlines()) if writes.exists() else 0
 def current_writes():return [json.loads(line) for line in writes.read_text().splitlines()] if writes.exists() else []
 position=[0,0]
 def move(x,y):
  pointer.stdin.write('m %.5f %.5f\n'%(x-position[0],y-position[1]));pointer.stdin.flush();position[:]=[x,y]
 def center(key):
  state=ui();index=state['keys'].index(key);return ((1280-state['width'])/2+12+(index+.5)*state['slot'],900-7-state['shelf']/2)
 def point(key):
  move(*center(key));return wait(lambda:(v if (v:=ui())['hovered']>=0 and v['keys'][v['hovered']]==key and v['contains'] else None))
 def button(value):pointer.stdin.write(value+'\n');pointer.stdin.flush()
 def drag(source,target):
  point(source);button('p');wait(lambda:ui()['pressed']);move(*center(target));wait(lambda:ui()['dragging'] and ui()['keys'][ui()['hovered']]==target)
 def settled(state):return all(abs(tile['width']-(tile['base']+18*tile['magnification']))<.15 for tile in state['tiles'])
 checks=[];sweeps=[]
 try:
  start_private_bus(env,a.output,processes)
  config=private/'sway.conf';config.write_text('output HEADLESS-1 resolution 1280x900 position 0 0\noutput HEADLESS-2 disable\nseat seat0 fallback true\nxwayland disable\nfocus_follows_mouse no\nfor_window [all] floating enable, border normal 1\n')
  launch([str(a.sway),'--unsupported-gpu','-c',str(config)],'sway.log');env['SWAYSOCK']=str(wait(lambda:next(Path(env['XDG_RUNTIME_DIR']).glob('sway-ipc*.sock'),None)));env['WAYLAND_DISPLAY']=wait(lambda:next((x.name for x in Path(env['XDG_RUNTIME_DIR']).glob('wayland-*') if not x.name.endswith('.lock')),None))
  pointer=launch([str(a.pointer),'pointer','move','0','0'],'pointer.log',stdin=sp.PIPE,text=True);pointer.stdin.write('m 0 0\n');pointer.stdin.flush();wait(lambda:any(seat.get('capabilities',0)&1 for seat in sway('-t','get_seats')));checked('seat seat0 cursor set 0 0');pointer.stdin.write('m 0 0\n');pointer.stdin.flush()
  env.update(LUMEN_LAUNCH_SCOPES='0',DBUS_SYSTEM_BUS_ADDRESS='unix:path='+str(private/'absent-system'),PULSE_SERVER='unix:'+str(private/'absent-audio'))
  assets=private/'assets';shutil.copytree(repo/'res/lumen-desktop',assets);writes=a.output/'writes.jsonl';store=private/'preferences.json';bin=private/'bin';bin.mkdir();service=bin/'lumen-appearance'
  service.write_text('#!'+sys.executable+'\n'+r'''import json,sys
from pathlib import Path
store=Path('''+repr(str(store))+r''');writes=Path('''+repr(str(writes))+r''')
initial={"dockPins":["fixture-a","fixture-b","fixture-c","unknown-retained"],"dockFolders":[{"path":"/synthetic/folder","view":"list"}],"dockFiles":[]}
data=json.loads(store.read_text()) if store.exists() else initial
if sys.argv[1]=='--set':
 key=sys.argv[2];value=json.loads(sys.argv[3]);data[key]=value
 with writes.open('a') as out:out.write(json.dumps({"key":key,"value":value})+'\n')
 store.write_text(json.dumps(data))
print(json.dumps({"preferences":data,"appearance":{},"system":{}}))
''');service.chmod(0o700);env['PATH']=str(bin)+os.pathsep+env['PATH']
  target=assets/'shell/Dock.qml';original=target.read_text();needle='    id: dock';assert original.count(needle)==1
  addition='''
    property alias fixturePointer:pointerArea
    property alias fixtureTip:tip
    property alias fixtureTipWindow:fixtureTooltipWindow
    function fixtureTiles(){
        const result=[];
        function visit(item){if(item.magnification!==undefined && item.modelData && item.index!==undefined){const icon=item.children.find(child=>child.iconSource!==undefined);result.push({key:item.modelData.key,index:item.index,magnification:item.magnification,width:icon.width,base:item.modelData.canvas*dock.artworkScale});}for(const child of item.children || [])visit(child);}
        visit(surface);return result.sort((a,b)=>a.index-b.index);
    }
'''
  tooltip='        screen:dock.screen;visible:dock.displayAllowed && dock.hovered>=0';assert original.count(tooltip)==1;modified=original.replace(needle,needle+addition,1).replace(tooltip,'        id:fixtureTooltipWindow\n'+tooltip,1);target.chmod(0o600);target.write_text(modified)
  import hashlib
  (a.output/'instrumentation.json').write_text(json.dumps(dict(originalSha256=hashlib.sha256(original.encode()).hexdigest(),modifiedSha256=hashlib.sha256(modified.encode()).hexdigest(),roundtrip=modified.replace(addition,'',1).replace('        id:fixtureTooltipWindow\n','',1)==original,scope='Read-only pointer/tooltip aliases, tile geometry collector, and tooltip id only. Production hit testing, hover, magnification, drag and release logic unchanged.'),indent=2))
  palette=json.loads((assets/'appearance/palette.json').read_text());appearance=private/'appearance.json';appearance.write_text(json.dumps(dict(font=palette['font'],palette=palette['dark'],preferences=dict(dockSize=48,dockMagnification=True,reduceMotion=False,reduceTransparency=True))));env['LUMEN_APPEARANCE_STATE']=str(appearance)
  qml=private/'dock.qml';qml.write_text('''import QtQuick
import Quickshell
import Quickshell.Io
import "assets/shell" as D
import "assets/shell/DockModel.js" as Model
ShellRoot {
 D.SettingsState {id:settings;active:true}
 QtObject {id:stub
  property var apps:[{id:"fixture-a",name:"Fixture A",icon:"application-x-executable"},{id:"fixture-b",name:"Fixture B",icon:"application-x-executable"},{id:"fixture-c",name:"Fixture C",icon:"application-x-executable"},{id:"fixture-recent",name:"Recent",icon:"application-x-executable"}]
  property var reference:[{key:"files",action:"files",image:"files.svg",canvas:68},{key:"a",appId:"fixture-a",canvas:68},{key:"b",appId:"fixture-b",canvas:68},{key:"c",appId:"fixture-c",canvas:68},{key:"downloads",action:"downloads",image:"files.svg",canvas:68},{key:"trash",action:"trash",image:"files.svg",canvas:68}]
  property bool dockPreferencesBusy:settings.busy
  property var referenceDock:Model.build(reference,apps,[],settings.preferences.dockPins,[apps[3]],true,settings.preferences.dockFolders,settings.preferences.dockFiles)
  property string popupOwner:""
  property date currentDate:new Date()
  property var activations:[]
  property string aliasError:""
  property string applicationCommandError:""
  signal requestDockFocus(string name)
  signal dismissPopups(string owner)
  function claimPopup(owner){popupOwner=owner;dismissPopups(owner);}
  function dockWindows(item){return [];}
  function dockLabel(item){return item.key;}
  function runningFor(entry){return false;}
  function activateDock(item,shift,name){activations=activations.concat(item.key);}
  function canConfigureDockEntry(entry){return entry.id!=="lumen-files";}
  function isDockPinned(entry){return (settings.preferences.dockPins || []).includes(entry.id);}
  function updateDockPin(entry,action){throw Error("Unexpected pin removal");}
  function reorderDockPin(source,target){if(!dockPreferencesBusy)settings.setPreference("dockPins",Model.movePin(settings.preferences.dockPins,source.id,target.id));}
  function removeDockAlias(item){throw Error("Unexpected alias removal");}
 }
 D.Dock {id:dock;shell:stub;screen:Quickshell.screens[0]}
 IpcHandler {target:"dockAudit"
  function status():string{return JSON.stringify({keys:stub.referenceDock.map(x=>x.key),separators:stub.referenceDock.map(x=>x.separatorBefore),width:dock.width,height:dock.height,slot:dock.slot,shelf:dock.shelfThickness,dragging:dock.dragging,hovered:dock.hovered,pointerSlot:dock.pointerSlot,contains:dock.fixturePointer.containsMouse,pressed:dock.fixturePointer.pressed,tip:dock.fixtureTip.text,tipVisible:dock.fixtureTipWindow.visible,tiles:dock.fixtureTiles(),activations:stub.activations,busy:settings.busy,ready:settings.available,pins:settings.preferences.dockPins});}
 }
}''')
  shell=launch(['quickshell','-p',str(qml)],'shell.log');wait(lambda:ui()['ready'] and not ui()['busy'] and len(ui()['tiles'])==len(ui()['keys']))
  originalPins=ui()['pins'];keys=ui()['keys'];assert ui()['separators'][keys.index('fixture-recent')] and ui()['separators'][keys.index('folder:/synthetic/folder')]
  for delay,reverse in ((.04,False),(.016,True),(.004,False)):
   order=list(reversed(keys)) if reverse else keys
   start=center(order[0]);move(*start);wait(lambda:ui()['hovered']==keys.index(order[0]));samples=[]
   for key in order[1:]:
    targetX,targetY=center(key);steps=max(1,int(abs(targetX-position[0])/8));dx=(targetX-position[0])/steps;dy=(targetY-position[1])/steps
    for step in range(steps):pointer.stdin.write('m %.5f %.5f\n'%(dx,dy));pointer.stdin.flush();position[0]+=dx;position[1]+=dy;time.sleep(delay)
    state=wait(lambda:(v if (v:=ui())['hovered']==keys.index(key) and v['tipVisible'] and v['tip']==key else None));samples.append(dict(key=key,hovered=state['hovered'],pointerSlot=state['pointerSlot'],tip=state['tip'],tiles=state['tiles']))
   state=wait(lambda:(v if settled(v:=ui()) else None));assert state['hovered']==keys.index(order[-1]) and abs(state['pointerSlot']-state['hovered'])<.03
   sweeps.append(dict(stepDelay=delay,reverse=reverse,samples=samples,settled=state));move(30,400);wait(lambda:ui()['hovered']==-1 and ui()['pointerSlot']==-1 and not ui()['tipVisible'] and ui()['tip']=='');wait(lambda:settled(ui()));assert all(abs(t['width']-t['base'])<.15 for t in ui()['tiles'])
  assert write_count()==0 and ui()['activations']==[];checks.append('Three actual continuous sweeps at varied motion rates update every neighbour tooltip and endpoint magnification, then clear without actions')
  count=write_count();drag('a','c');button('r');expected=['fixture-b','fixture-a','fixture-c','unknown-retained'];wait(lambda:ui()['pins']==expected and not ui()['busy']);assert write_count()==count+1 and current_writes()[-1]==dict(key='dockPins',value=expected) and not ui()['activations'];checks.append('Physical app reorder adjacent to recent/document separators emits exactly one captured pin-order write and preserves unknown pins')
  os.killpg(shell.pid,signal.SIGTERM);shell.wait(timeout=5);move(30,400);shell=launch(['quickshell','-p',str(qml)],'shell-restart.log');wait(lambda:ui()['ready'] and not ui()['busy']);assert ui()['pins']==expected and ui()['keys'].index('b')<ui()['keys'].index('a')<ui()['keys'].index('c');checks.append('Private production Dock/SettingsState restart reconstructs the captured order from the synthetic persisted settings transport')
  count=write_count();drag('c','folder:/synthetic/folder');button('r');wait(lambda:not ui()['dragging']);assert write_count()==count and ui()['pins']==expected and not ui()['activations'];checks.append('Dropping a held app over document section does not reorder, remove, activate, or retarget an alias')
  count=write_count();drag('c','b');button('r');expected=['fixture-c','fixture-b','fixture-a','unknown-retained'];wait(lambda:ui()['pins']==expected and not ui()['busy']);assert write_count()==count+1 and current_writes()[-1]['value']==expected;checks.append('Reverse physical reorder uses the captured source and destination after reconstruction')
  call('grim',a.output/'accepted.png')
  logs=''.join(path.read_text() for path in a.output.glob('shell*.log'));assert not any(word in logs for word in ('ReferenceError','TypeError','Binding loop','Failed to load')),logs
  (a.output/'result.json').write_text(json.dumps(dict(passed=True,checks=checks,source=a.source_revision,compositor=str(a.sway),sweeps=sweeps,writes=current_writes(),final=ui(),settingsScope='Fixed synthetic settings transport; actual production Dock/SettingsState/model and captured writes. Not production configuration/Nix persistence.',liveSocketUnchanged=os.environ.get('SWAYSOCK')==original_socket),indent=2));print(json.dumps(dict(passed=True,checks=len(checks))))
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

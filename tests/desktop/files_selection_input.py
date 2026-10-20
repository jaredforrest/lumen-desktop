"""Actual installed Files viewport band selection, private paths and physical input."""
from fixture_resources import close_reaped_pipes
import argparse,hashlib,json,os,re,shutil,signal,subprocess as sp,sys,tempfile,time
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--launcher',type=Path,required=True);p.add_argument('--sway',required=True);p.add_argument('--pointer-tool',required=True);p.add_argument('--effects',action='store_true');p.add_argument('--output',type=Path,required=True);p.add_argument('--inside',action='store_true');a=p.parse_args()
if not a.inside:
 with tempfile.TemporaryDirectory(prefix='lumen-selection-input-') as private:
  env=dict(os.environ,HOME=private,XDG_RUNTIME_DIR=private,DBUS_SYSTEM_BUS_ADDRESS='unix:path='+private+'/no-system-bus')
  for key in ('CONFIG','STATE','CACHE','DATA'):env['XDG_'+key+'_HOME']=str(Path(private)/key.lower())
  for key in ('DISPLAY','WAYLAND_DISPLAY','SWAYSOCK','DBUS_SESSION_BUS_ADDRESS','AT_SPI_BUS_ADDRESS'):env.pop(key,None)
  raise SystemExit(sp.call(['dbus-run-session','--',sys.executable,__file__,*sys.argv[1:],'--inside'],env=env))
repo=Path(__file__).resolve().parents[2];out=a.output.resolve();out.mkdir(parents=True,exist_ok=False);processes=[];checks=[]
prefix=re.split(r'\n[ \t]*exec quickshell ',a.launcher.read_text(),maxsplit=1)[0]
env=dict(value.decode().split('=',1) for value in sp.check_output(['bash','-c',prefix+'\nexec env -0']).split(b'\0') if b'=' in value)
env.update(WLR_BACKENDS='headless',WLR_RENDERER='gles2' if a.effects else 'pixman',WLR_RENDERER_FORCE_SOFTWARE='1' if a.effects else '0',WLR_RENDERER_ALLOW_SOFTWARE='1',LIBGL_ALWAYS_SOFTWARE='1',WLR_LIBINPUT_NO_DEVICES='1',QT_QPA_PLATFORM='wayland',QT_QPA_PLATFORMTHEME='generic',QT_QUICK_BACKEND='software',QT_ACCESSIBILITY='0',NO_AT_BRIDGE='1',GTK_A11Y='none',LUMEN_FILE_SERVICE='0',LUMEN_LAUNCH_SCOPES='0',LUMEN_NATIVE_WINDOW_CONTROLS='0',LUMEN_NATIVE_WINDOW_ACTIONS='0',LUMEN_UNIFIED_OWNED_HEADERS='1')
for key in ('DISPLAY','WAYLAND_DISPLAY','SWAYSOCK','AT_SPI_BUS_ADDRESS'):env.pop(key,None)
env.update(PULSE_SERVER='unix:'+env['XDG_RUNTIME_DIR']+'/no-audio',PIPEWIRE_RUNTIME_DIR=env['XDG_RUNTIME_DIR'],PIPEWIRE_REMOTE='no-pipewire')
docs=Path(env['HOME'])/'Documents';docs.mkdir()
for index in range(160):(docs/f'item-{index:03}.txt').write_text('synthetic '+str(index))
palettes=json.loads((repo/'res/lumen-desktop/appearance/palette.json').read_text());appearance=out/'appearance.json';appearance.write_text(json.dumps(dict(font=palettes['font'],palette=palettes['dark'],preferences={})));env['LUMEN_APPEARANCE_STATE']=str(appearance)
assets=out/'assets';shutil.copytree(Path(env['LUMEN_NOTIFICATION_HELPER']).parent.parent,assets)
backend=assets/'shell/files.py';original=backend.read_text();needle="    if action=='volumes':\n";assert original.count(needle)==1
backend.chmod(0o600);backend.write_text(original.replace(needle,"    if action=='volumes':return dict(volumes=[])\n"+needle))
(out/'isolation.json').write_text(json.dumps(dict(originalSha256=hashlib.sha256(original.encode()).hexdigest(),modifiedSha256=hashlib.sha256(backend.read_bytes()).hexdigest(),seam=needle,reason='Disable live mount discovery only; all selection paths and assertions preserved'),indent=2))
qml=out/'shell.qml';qml.write_text('''import QtQuick
import QtQuick.Window
import Quickshell
import Quickshell.Io
import "assets/shell" as D
ShellRoot {
 id:fixture
 QtObject {id:exported;property var state:({appName:"",title:"",windowId:0})}
 QtObject {id:permission;property bool permitted:true}
 QtObject {id:stub;property int focusEpoch:0;property var windows:[];property var exportedMenu:exported;property var screenSaverPreferences:permission;function activeFiles(window){}function focusWindow(window){}}
 D.FilesState {id:service;persistViewPreferences:false;preferencesLoaded:true;path:'''+json.dumps(str(docs))+''';viewMode:"list"}
 D.FilesWindow {id:files;shell:stub;service:service;visible:true}
 Component.onCompleted:service.navigate(service.path)
 function find(name,item){if(item.objectName===name)return item;for(const child of item.children || []){const found=find(name,child);if(found)return found;}return null;}
 function band(){return find("files-"+(service.viewMode==="columns"?"column-band-"+service.path:service.viewMode+"-band-selection"),files.contentItem);}
 IpcHandler {target:"selectionAudit"
  function mode(value:string):void {service.viewMode=value;}
  function refresh():void {service.refresh();}
  function reverse():void {service.descending=!service.descending;service.refresh();}
  function state():string {const b=fixture.band();return JSON.stringify({ready:service.ready && !service.busy && service.rows.length===160,mode:service.viewMode,selection:files.selectedPaths,preview:files.bandPreviewPaths,current:files.current?files.current.path:null,pending:!!b && b.pending,active:!!b && b.active,error:service.error});}
  function geometry():string {const b=fixture.band();if(!b || !b.permitted)return "null";const boxes=b.boxes().filter(box=>box.x>=b.view.contentX && box.y>=b.view.contentY && box.x+box.width<b.view.contentX+b.width && box.y+box.height<b.view.contentY+b.height);if(!boxes.length)return "null";const first=boxes[0],at=b.mapToItem(files.contentItem,0,0),horizontal=service.viewMode==="gallery";const start=horizontal?{x:first.x+first.width/2,y:b.view.contentY+b.height-b.bottomInset-6}:{x:first.x+first.width+6,y:first.y+first.height/2};const end={x:first.x+1,y:first.y+1};return JSON.stringify({start:{x:at.x+start.x-b.view.contentX,y:at.y+start.y-b.view.contentY},end:{x:at.x+end.x-b.view.contentX,y:at.y+end.y-b.view.contentY},row:{x:at.x+first.x-b.view.contentX+first.width/2,y:at.y+first.y-b.view.contentY+first.height/2},path:first.row.path,populated:horizontal?b.view.contentWidth>b.width:b.view.contentHeight>b.height});}
 }
}''')
def launch(command,name):
 with (out/name).open('w') as log:process=sp.Popen(command,env=env,stdout=log,stderr=sp.STDOUT,start_new_session=True)
 processes.append(process);return process
def call(*command):return sp.check_output(command,env=env,text=True,timeout=10).strip()
def ipc(method,*values):return call('quickshell','-p',str(qml),'ipc','call','selectionAudit',method,*values)
def state():return json.loads(ipc('state'))
def wait(fn):
 end=time.monotonic()+20
 while time.monotonic()<end:
  try:
   value=fn()
   if value:return value
  except (OSError,ValueError,sp.SubprocessError):pass
  time.sleep(.08)
 try:(out/'failed-state.json').write_text(json.dumps(state(),indent=2))
 except Exception:pass
 raise AssertionError('Selection input condition timed out; inspect retained logs/state')
def nodes(node):
 yield node
 for child in node.get('nodes',[])+node.get('floating_nodes',[]):yield from nodes(child)
def window():return next((node for node in nodes(json.loads(call('swaymsg','-r','-t','get_tree'))) if node.get('app_id')=='org.quickshell' and node.get('name','').startswith('Files')),None)
def key(*values):call('wtype',*values);time.sleep(.12)
def point(value):
 native=window()['rect'];call('swaymsg','seat','seat0','cursor','set',str(round(native['x']+value['x'])),str(round(native['y']+value['y'])));pointer.stdin.write('m 0.5 0.5\n');pointer.stdin.flush();time.sleep(.1)
def button(value):pointer.stdin.write(value+'\n');pointer.stdin.flush();time.sleep(.12)
def begin(box):
 point(box['start']);button('p');wait(lambda:state()['pending']);point(box['end']);wait(lambda:state()['active'])
try:
 config=out/'sway.conf';config.write_text('output HEADLESS-1 resolution 1280x900\nxwayland disable\nfor_window [all] floating enable, border pixel 0\n')
 launch([a.sway,'--unsupported-gpu','-c',str(config)],'sway.log');runtime=Path(env['XDG_RUNTIME_DIR']);env['SWAYSOCK']=str(wait(lambda:next(runtime.glob('sway-ipc*.sock'),None)));env['WAYLAND_DISPLAY']=wait(lambda:next((path.name for path in runtime.glob('wayland-*') if not path.name.endswith('.lock')),None))
 launch(['quickshell','-p',str(qml)],'shell.log');wait(lambda:state()['ready']);native=wait(window);call('swaymsg','[con_id='+str(native['id'])+'] focus');launch(['wtype','-s','60000'],'keyboard.log')
 pointer=sp.Popen([a.pointer_tool,'pointer','move','0','0'],env=env,stdin=sp.PIPE,text=True,start_new_session=True);processes.append(pointer)
 for mode in ('list','grid','gallery','columns'):
  ipc('mode',mode);box=wait(lambda:json.loads(ipc('geometry')));assert box['populated'],box
  before=state()['selection'];begin(box);assert state()['selection']==before and state()['preview'],state();button('r');wait(lambda:not state()['pending']);assert box['path'] in state()['selection'];checks.append(mode+': actual blank gutter starts preview without writes; release selects exact row')
  before=state()['selection'];begin(box);key('-k','Escape');button('r');assert not state()['pending'] and state()['selection']==before and state()['preview'] is None;checks.append(mode+': physical Escape cancels without committing preview')
  key('-M','ctrl');begin(box);button('r');key('-m','ctrl');wait(lambda:not state()['pending']);assert box['path'] not in state()['selection'];checks.append(mode+': physical Control band toggles captured membership')
  key('-M','shift');begin(box);button('r');key('-m','shift');wait(lambda:not state()['pending']);assert box['path'] in state()['selection'];checks.append(mode+': physical Shift band adds exact captured membership')
  point(box['row']);button('p');button('r');wait(lambda:state()['selection']==[box['path']]);checks.append(mode+': ordinary delegate pointer selection remains available')
  before=state()['selection'];begin(box);ipc('reverse');wait(lambda:not state()['pending'] and state()['ready']);button('r');assert state()['selection']==before and state()['preview'] is None;checks.append(mode+': actual resort cancels held geometry without selecting another object')
  box=wait(lambda:json.loads(ipc('geometry')));before=state()['selection'];begin(box);pointer.stdin.write('w 1\n');pointer.stdin.flush();wait(lambda:not state()['pending']);button('r');assert state()['selection']==before and state()['preview'] is None;checks.append(mode+': physical wheel cancels held viewport preview')
 call('grim',str(out/'selection.png'))
 log=(out/'shell.log').read_text();assert not any(marker in log for marker in ('ReferenceError:','TypeError:','Binding loop','Failed to load configuration','Cannot assign','Accessible attached property must')),log
 (out/'result.json').write_text(json.dumps(dict(passed=True,checks=checks,source=call('git','-C',str(repo),'rev-parse','HEAD'),launcher=str(a.launcher),sway=a.sway,effects=a.effects,fixtureSha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),assetsSha256={str(path.relative_to(assets)):hashlib.sha256(path.read_bytes()).hexdigest() for path in sorted(assets.rglob('*')) if path.is_file()}),indent=2))
finally:
 for process in reversed(processes):
  if process.poll() is None:os.killpg(process.pid,signal.SIGTERM)
 for process in reversed(processes):
  try:process.wait(timeout=5)
  except sp.TimeoutExpired:os.killpg(process.pid,signal.SIGKILL);process.wait(timeout=5)
 close_reaped_pipes(processes)

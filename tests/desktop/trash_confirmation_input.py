"""Actual private Files confirmation input and retained personal-Trash outcomes."""
from fixture_resources import close_reaped_pipes
import argparse,hashlib,json,os,re,shutil,signal,subprocess as sp,sys,tempfile,time
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--output',type=Path,required=True);p.add_argument('--launcher',type=Path,required=True);p.add_argument('--sway',required=True);p.add_argument('--pointer-tool',required=True);p.add_argument('--effects',action='store_true');p.add_argument('--inside',action='store_true');a=p.parse_args()
if not a.inside:
 with tempfile.TemporaryDirectory(prefix='lumen-trash-input-') as private:
  root=Path(private);env=dict(os.environ,HOME=private,XDG_RUNTIME_DIR=private)
  for key in ('CONFIG','STATE','CACHE','DATA'):env['XDG_'+key+'_HOME']=str(root/key.lower())
  for key in ('DISPLAY','WAYLAND_DISPLAY','SWAYSOCK','DBUS_SESSION_BUS_ADDRESS','AT_SPI_BUS_ADDRESS'):env.pop(key,None)
  config=root/'bus.conf';config.write_text('<busconfig><type>session</type><listen>unix:tmpdir='+private+'</listen><policy user="'+str(os.getuid())+'"><allow own="*"/><allow send_destination="*"/><allow receive_sender="*"/></policy></busconfig>')
  raise SystemExit(sp.call(['dbus-run-session','--config-file='+str(config),'--',sys.executable,__file__,*sys.argv[1:],'--inside'],env=env))
repo=Path(__file__).resolve().parents[2];out=a.output.resolve();out.mkdir(parents=True,exist_ok=False)
prefix=re.split(r'\n[ \t]*exec quickshell ',a.launcher.read_text(),maxsplit=1)[0]
exported=sp.check_output(['bash','-c',prefix+'\nexec env -0']);env=dict(value.decode().split('=',1) for value in exported.split(b'\0') if b'=' in value)
env.update(WLR_BACKENDS='headless',WLR_RENDERER='pixman',WLR_LIBINPUT_NO_DEVICES='1',QT_QPA_PLATFORM='wayland',QT_QPA_PLATFORMTHEME='generic',QT_QUICK_BACKEND='software',QT_ACCESSIBILITY='0',QT_LINUX_ACCESSIBILITY_ALWAYS_ON='0',NO_AT_BRIDGE='1',GTK_A11Y='none',LUMEN_FILE_SERVICE='1',LUMEN_LAUNCH_SCOPES='0',LUMEN_NATIVE_WINDOW_CONTROLS='0',LUMEN_NATIVE_WINDOW_ACTIONS='0',LUMEN_UNIFIED_OWNED_HEADERS='1',PYTHONWARNINGS='error')
env['DBUS_SYSTEM_BUS_ADDRESS']='unix:path='+str(Path(env['XDG_RUNTIME_DIR'])/'inert-system');env['PULSE_SERVER']='unix:'+str(Path(env['XDG_RUNTIME_DIR'])/'inert-audio')
for key in ('DISPLAY','WAYLAND_DISPLAY','SWAYSOCK','AT_SPI_BUS_ADDRESS'):env.pop(key,None)
if a.effects:env.update(WLR_RENDERER='gles2',WLR_RENDERER_FORCE_SOFTWARE='1',WLR_RENDERER_ALLOW_SOFTWARE='1',LIBGL_ALWAYS_SOFTWARE='1')
assets=out/'assets';shutil.copytree(repo/'res/lumen-desktop',assets)
# Current selection integration is an explicit source dependency, never a fallback.
assert 'identity=file_identity.wire(s)' in (assets/'shell/file_jobs.py').read_text(),'Requires canonical personal-Trash listing identities from selection integration'
trash=Path(env['XDG_DATA_HOME'])/'Trash'
for path in (trash,trash/'files',trash/'info'):path.mkdir(mode=0o700,parents=True,exist_ok=True)
def item(name):
 path=trash/'files'/name;path.write_text('disposable '+name)
 from urllib.parse import quote
 record=trash/'info'/(name+'.trashinfo');record.write_text('[Trash Info]\nPath='+quote(str(Path(env['HOME'])/name),safe='/')+'\n');record.chmod(0o600);return path
first=item('Literal # é.txt');second=item('Second.txt')
palettes=json.loads((assets/'appearance/palette.json').read_text());appearance=out/'appearance.json';appearance.write_text(json.dumps(dict(font=palettes['font'],palette=palettes['dark'],preferences={'textScale':1.5})));env['LUMEN_APPEARANCE_STATE']=str(appearance)
qml=out/'shell.qml';qml.write_text('''import QtQuick
import Quickshell
import Quickshell.Io
import "assets/shell" as D
ShellRoot {
 id:fixture
 QtObject {id:exported;property var state:({appName:"",title:"",windowId:0})}
 QtObject {id:stub;property int focusEpoch:0;property var windows:[];property var exportedMenu:exported;function activeFiles(window){}function focusWindow(window){}}
 D.FilesState {id:service;persistViewPreferences:false;path:"trash:";viewMode:"list"}
 D.FilesWindow {id:files;shell:stub;service:service;visible:true;implicitWidth:760;implicitHeight:560}
 function find(name,item){if(!item)return null;if(item.objectName===name)return item;for(const child of item.children || []){const result=find(name,child);if(result)return result;}return null;}
 function controller(){return find("trash-actions-state",files.contentItem);}
 function content(){return files.contentItem.Window.window.contentItem;}
 IpcHandler {target:"trashAudit"
  function state():string {const state=fixture.controller(),confirm=fixture.find("trash-confirm",fixture.content());return JSON.stringify({ready:service.ready && !service.busy,rows:service.rows.map(row=>({path:row.path,identity:row.identity})),error:service.error,controller:!!state,busy:state?state.busy:false,waiting:state?state.waiting:false,confirmation:state?state.confirmation:null,operations:state?state.operations:[],reason:state?state.error:"",confirmVisible:confirm?confirm.visible:false,focus:fixture.content().Window.window.activeFocusItem?fixture.content().Window.window.activeFocusItem.objectName:""});}
  function prepare(mode:string,path:string):void {files.selectedPaths=path?[path]:[];files.selected=service.rows.findIndex(row=>row.path===path);files.prepareDeletion(mode);}
  function refresh():void {service.refresh();}
  function closeResults():void {const done=fixture.find("trash-results-done",fixture.content());if(done)done.clicked();}
  function geometry(name:string):string {const item=fixture.find(name,fixture.content());if(!item)return "null";const at=item.mapToItem(files.contentItem,0,0);return JSON.stringify({x:at.x,y:at.y,width:item.width,height:item.height});}
 }
}''')
processes=[]
def launch(command,name):
 with (out/name).open('a') as log:process=sp.Popen(command,env=env,stdout=log,stderr=sp.STDOUT,start_new_session=True)
 processes.append(process);return process

def call(*command):return sp.check_output(command,env=env,text=True,timeout=8).strip()
def ipc(method,*values):return call('quickshell','-p',str(qml),'ipc','call','trashAudit',method,*values)
def state():return json.loads(ipc('state'))
def wait(fn):
 deadline=time.monotonic()+15
 while time.monotonic()<deadline:
  try:
   value=fn()
   if value:return value
  except (OSError,ValueError,sp.SubprocessError,StopIteration):pass
  time.sleep(.08)
 try:(out/'failed-state.json').write_text(json.dumps(state(),indent=2))
 except Exception:pass
 raise AssertionError('Private Trash native acceptance timed out; retained logs/state')
def nodes(node):
 yield node
 for child in node.get('nodes',[])+node.get('floating_nodes',[]):yield from nodes(child)
def window():return next(node for node in nodes(json.loads(call('swaymsg','-r','-t','get_tree'))) if node.get('app_id')=='org.quickshell' and node.get('name','').startswith('Files'))
def key(*keys):call('wtype',*keys);time.sleep(.12)
def click(name):
 geometry=json.loads(ipc('geometry',name));assert geometry,name;rect=window()['rect'];call('swaymsg','seat','seat0','cursor','set',str(round(rect['x']+geometry['x']+geometry['width']/2)),str(round(rect['y']+geometry['y']+geometry['height']/2)))
 pointer.stdin.write('m 0.5 0.5\np\nr\n');pointer.stdin.flush();time.sleep(.18)
checks=[]
try:
 config=out/'sway.conf';config.write_text('output HEADLESS-1 resolution 1000x760\nxwayland disable\nfor_window [all] floating enable, border pixel 0\n')
 launch([a.sway,'--unsupported-gpu','-c',str(config)],'sway.log');runtime=Path(env['XDG_RUNTIME_DIR']);env['SWAYSOCK']=str(wait(lambda:next(runtime.glob('sway-ipc*.sock'),None)));env['WAYLAND_DISPLAY']=wait(lambda:next((path.name for path in runtime.glob('wayland-*') if not path.name.endswith('.lock')),None))
 launch([sys.executable,str(assets/'shell/file_jobs.py')],'operations.log');wait(lambda:(runtime/'lumen-file-operations.sock').exists())
 shell=launch(['quickshell','-p',str(qml)],'shell.log');wait(lambda:state()['ready'] and state()['controller'] and len(state()['rows'])==2)
 launch(['wtype','-s','60000'],'keyboard.log');pointer=sp.Popen([a.pointer_tool,'pointer','move','0','0'],env=env,stdin=sp.PIPE,text=True,start_new_session=True);processes.append(pointer)
 ipc('prepare','selected',str(first));wait(lambda:state()['confirmation']);wait(lambda:state()['focus']=='trash-cancel');key('-k','Return');wait(lambda:not state()['confirmation']);assert first.exists();checks.append('Actual Return activates default Cancel without deleting captured item')
 ipc('prepare','selected',str(first));wait(lambda:state()['confirmation']);key('-k','Escape');wait(lambda:not state()['confirmation']);assert first.exists();checks.append('Actual Escape retires pending confirmation without deletion')
 ipc('prepare','empty','');wait(lambda:state()['confirmation']);late=item('Later.txt');call('grim',str(out/'confirmation-150-percent.png'));click('trash-confirm');wait(lambda:state()['operations'] and not state()['busy'] and state()['operations'][-1]['state']=='complete');assert not first.exists() and not second.exists() and late.exists();checks.append('Actual pointer confirms captured Empty Trash; later arrival excluded at enlarged text')
 ipc('closeResults');ipc('refresh');wait(lambda:state()['ready'] and len(state()['rows'])==1);ipc('prepare','selected',str(late));wait(lambda:state()['confirmation']);retired=late.with_name('retired.txt');late.rename(retired);late.write_text('replacement');click('trash-confirm');wait(lambda:state()['operations'] and not state()['busy'] and state()['operations'][-1]['state']=='partial');assert late.read_text()=='replacement' and retired.exists();checks.append('Replaced selected path produces truthful kept/partial outcome without deleting replacement')
 before=state()['operations'][-1];os.killpg(shell.pid,signal.SIGTERM);shell.wait(timeout=5);shell=launch(['quickshell','-p',str(qml)],'shell.log');wait(lambda:state()['ready'] and state()['controller'] and state()['operations']);assert state()['operations'][-1]['id']==before['id'] and late.read_text()=='replacement';checks.append('Private shell restart reads retained outcome and never replays destructive confirmation')
 log=(out/'shell.log').read_text();assert not any(marker in log for marker in ('ReferenceError:','TypeError:','Binding loop','Failed to load configuration','Cannot assign','Unable to assign')),log
 manifest=dict(passed=True,checks=checks,final=state(),launcher=str(a.launcher),sway=str(a.sway),effects=a.effects,source={str(path.relative_to(repo)):hashlib.sha256(path.read_bytes()).hexdigest() for path in [repo/'res/lumen-desktop/shell/FilesWindow.qml',repo/'res/lumen-desktop/shell/file_jobs.py',repo/'res/lumen-desktop/shell/trash_operations.py',Path(__file__).resolve()]},isolation='Disposable HOME/XDG/personal Trash, activation-free private session bus, inert system/audio, only owned Files fragment and private operation service')
 (out/'result.json').write_text(json.dumps(manifest,indent=2))
finally:
 for process in reversed(processes):
  if process.poll() is None:os.killpg(process.pid,signal.SIGTERM)
 for process in reversed(processes):
  try:process.wait(timeout=5)
  except sp.TimeoutExpired:os.killpg(process.pid,signal.SIGKILL);process.wait(timeout=5)
 close_reaped_pipes(processes)

"""Actual installed Files batch dialog, private paths and physical input."""
from fixture_resources import close_reaped_pipes
import argparse,hashlib,json,os,re,shutil,signal,subprocess as sp,sys,tempfile,time
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--launcher',type=Path,required=True);p.add_argument('--sway',required=True);p.add_argument('--pointer-tool',required=True);p.add_argument('--effects',action='store_true');p.add_argument('--output',type=Path,required=True);p.add_argument('--inside',action='store_true');a=p.parse_args()
if not a.inside:
 with tempfile.TemporaryDirectory(prefix='lumen-batch-input-') as private:
  env=dict(os.environ,HOME=private,XDG_RUNTIME_DIR=private,DBUS_SYSTEM_BUS_ADDRESS='unix:path='+private+'/no-system-bus')
  for key in ('CONFIG','STATE','CACHE','DATA'):env['XDG_'+key+'_HOME']=str(Path(private)/key.lower())
  for key in ('DISPLAY','WAYLAND_DISPLAY','SWAYSOCK','DBUS_SESSION_BUS_ADDRESS','AT_SPI_BUS_ADDRESS'):env.pop(key,None)
  raise SystemExit(sp.call(['dbus-run-session','--',sys.executable,__file__,*sys.argv[1:],'--inside'],env=env))
repo=Path(__file__).resolve().parents[2];out=a.output.resolve();out.mkdir(parents=True,exist_ok=False);processes=[];checks=[]
prefix=re.split(r'\n[ \t]*exec quickshell ',a.launcher.read_text(),maxsplit=1)[0]
env=dict(value.decode().split('=',1) for value in sp.check_output(['bash','-c',prefix+'\nexec env -0']).split(b'\0') if b'=' in value)
env.update(WLR_BACKENDS='headless',WLR_RENDERER='gles2' if a.effects else 'pixman',WLR_RENDERER_FORCE_SOFTWARE='1' if a.effects else '0',WLR_RENDERER_ALLOW_SOFTWARE='1',LIBGL_ALWAYS_SOFTWARE='1',WLR_LIBINPUT_NO_DEVICES='1',QT_QPA_PLATFORM='wayland',QT_QPA_PLATFORMTHEME='generic',QT_QUICK_BACKEND='software',QT_ACCESSIBILITY='0',NO_AT_BRIDGE='1',GTK_A11Y='none',LUMEN_FILE_SERVICE='0',LUMEN_LAUNCH_SCOPES='0',LUMEN_NATIVE_WINDOW_CONTROLS='0',LUMEN_NATIVE_WINDOW_ACTIONS='0',LUMEN_UNIFIED_OWNED_HEADERS='1')
for key in ('DISPLAY','WAYLAND_DISPLAY','SWAYSOCK','AT_SPI_BUS_ADDRESS'):env.pop(key,None)
docs=Path(env['HOME'])/'Documents';docs.mkdir();(docs/'lower-one.txt').write_text('one');(docs/'lower-two.txt').write_text('two')
palettes=json.loads((repo/'res/lumen-desktop/appearance/palette.json').read_text());appearance=out/'appearance.json';appearance.write_text(json.dumps(dict(font=palettes['font'],palette=palettes['dark'],preferences={})));env['LUMEN_APPEARANCE_STATE']=str(appearance)
assets=out/'assets';shutil.copytree(Path(env['LUMEN_NOTIFICATION_HELPER']).parent.parent,assets)
backend=assets/'shell/files.py';original=backend.read_text();needle="    if action=='volumes':\n";assert original.count(needle)==1
backend.chmod(0o600);backend.write_text(original.replace(needle,"    if action=='volumes':return dict(volumes=[])\n"+needle))
(out/'isolation.json').write_text(json.dumps(dict(originalSha256=hashlib.sha256(original.encode()).hexdigest(),modifiedSha256=hashlib.sha256(backend.read_bytes()).hexdigest(),seam=needle,reason='Disable live mount discovery only; all rename paths and assertions preserved'),indent=2))
qml=out/'shell.qml';qml.write_text('''import QtQuick
import QtQuick.Window
import Quickshell
import Quickshell.Io
import "assets/shell" as D
ShellRoot {
 id:fixture
 property var latest:({})
 QtObject {id:exported;property var state:({appName:"",title:"",windowId:0})}
 QtObject {id:stub;property int focusEpoch:0;property var windows:[];property var exportedMenu:exported;function activeFiles(window){}function focusWindow(window){}}
 D.FilesState {id:service;persistViewPreferences:false;preferencesLoaded:true;path:'''+json.dumps(str(docs))+''';viewMode:"list";onBatchRenameReady:reply=>{if(reply.action!=="rename-cancel")fixture.latest=reply}}
 D.FilesWindow {id:files;shell:stub;service:service;visible:true}
 Component.onCompleted:service.navigate(service.path)
 function tree(){const window=files.contentItem.Window.window;return window?window.contentItem:files.contentItem;}
 function find(name,item){if(item.objectName===name)return item;for(const child of item.children || []){const found=find(name,child);if(found)return found;}return null;}
 IpcHandler {target:"batchAudit"
  function state():string {const confirm=fixture.find("batch-rename-confirm",fixture.tree()),first=fixture.find("batch-rename-first",fixture.tree());return JSON.stringify({ready:service.ready && !service.busy && service.rows.length>0,rows:service.rows,selection:files.selectedPaths,reply:fixture.latest,open:!!first && first.visible,canConfirm:!!confirm && confirm.enabled,error:service.error});}
  function geometry(name:string):string {const item=fixture.find(name,fixture.tree());if(!item)return "null";const at=item.mapToItem(files.contentItem,0,0);return JSON.stringify({x:at.x,y:at.y,width:item.width,height:item.height,visible:item.visible});}
  function refresh():void {service.refresh();}
 }
}''')
def launch(command,name):
 with (out/name).open('w') as log:process=sp.Popen(command,env=env,stdout=log,stderr=sp.STDOUT,start_new_session=True)
 processes.append(process);return process
def call(*command):return sp.check_output(command,env=env,text=True,timeout=10).strip()
def ipc(method,*values):return call('quickshell','-p',str(qml),'ipc','call','batchAudit',method,*values)
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
 raise AssertionError('Batch input condition timed out; inspect retained logs/state')
def nodes(node):
 yield node
 for child in node.get('nodes',[])+node.get('floating_nodes',[]):yield from nodes(child)
def window():return next((node for node in nodes(json.loads(call('swaymsg','-r','-t','get_tree'))) if node.get('app_id')=='org.quickshell' and node.get('name','').startswith('Files')),None)
def key(*values):call('wtype',*values);time.sleep(.12)
def click(name):
 box=wait(lambda:json.loads(ipc('geometry',name)));assert box['visible'],(name,box);native=window()['rect'];call('swaymsg','seat','seat0','cursor','set',str(round(native['x']+box['x']+box['width']/2)),str(round(native['y']+box['y']+box['height']/2)));pointer.stdin.write('m 0.5 0.5\np\nr\n');pointer.stdin.flush();time.sleep(.15)
def field(name,text):click(name);key('-M','ctrl','-k','a','-m','ctrl');key(text)
def open_batch():
 wait(lambda:state()['ready']);key('-M','ctrl','-k','a','-m','ctrl');wait(lambda:len(state()['selection'])==2);key('-k','F2');wait(lambda:state()['open'])
def preview():
 previous=state()['reply'].get('id',0);click('batch-rename-preview-button');return wait(lambda:state()['reply'].get('action')=='rename-preview' and state()['reply'].get('id',0)>previous and not state()['error'])
try:
 config=out/'sway.conf';config.write_text('output HEADLESS-1 resolution 1280x900\nxwayland disable\nfor_window [all] floating enable, border pixel 0\n')
 launch([a.sway,'--unsupported-gpu','-c',str(config)],'sway.log');runtime=Path(env['XDG_RUNTIME_DIR']);env['SWAYSOCK']=str(wait(lambda:next(runtime.glob('sway-ipc*.sock'),None)));env['WAYLAND_DISPLAY']=wait(lambda:next((path.name for path in runtime.glob('wayland-*') if not path.name.endswith('.lock')),None))
 launch(['quickshell','-p',str(qml)],'shell.log');wait(lambda:state()['ready']);native=wait(window);call('swaymsg','[con_id='+str(native['id'])+'] focus');launch(['wtype','-s','60000'],'keyboard.log')
 pointer=sp.Popen([a.pointer_tool,'pointer','move','0','0'],env=env,stdin=sp.PIPE,text=True,start_new_session=True);processes.append(pointer)
 open_batch();field('batch-rename-first','lower');field('batch-rename-second','LOWER');before=sorted(path.name for path in docs.iterdir());preview();assert state()['canConfirm'] and sorted(path.name for path in docs.iterdir())==before;call('grim',str(out/'preview.png'));checks.append('Actual Ctrl+A/F2 opens captured sibling preview and writes no files')
 # Pointer activated Preview; the next focusable enabled action is confirmation.
 started=time.monotonic();key('-k','Tab','-k','Return');wait(lambda:state()['reply'].get('action')=='rename-confirm');confirmationLatency=time.monotonic()-started;assert confirmationLatency<=5,confirmationLatency;assert (docs/'LOWER-one.txt').read_text()=='one' and (docs/'LOWER-two.txt').read_text()=='two';assert all(row['status']=='renamed' for row in state()['reply']['rows']);checks.append('Actual keyboard confirmation commits case-only changes and exposes per-item results')
 click('batch-rename-cancel');wait(lambda:not state()['open']);wait(lambda:state()['ready']);open_batch();field('batch-rename-first','LOWER');field('batch-rename-second','UPPER');preview();key('-k','Escape');wait(lambda:not state()['open']);assert not (docs/'UPPER-one.txt').exists();checks.append('Escape cancels a prepared preview without any rename')
 open_batch();field('batch-rename-first','LOWER');field('batch-rename-second','UPPER');preview();replacement=docs/'replacement';replacement.write_text('replacement');replacement.replace(docs/'LOWER-two.txt');click('batch-rename-confirm');wait(lambda:state()['reply'].get('action')=='rename-confirm');assert state()['reply'].get('error') and not (docs/'UPPER-one.txt').exists() and (docs/'LOWER-two.txt').read_text()=='replacement';checks.append('Source replacement after actual preview rejects the entire confirmation')
 click('batch-rename-cancel');wait(lambda:not state()['open']);open_batch();field('batch-rename-first','LOWER');field('batch-rename-second','UPPER');preview();(docs/'UPPER-two.txt').write_text('bystander');click('batch-rename-confirm');wait(lambda:state()['reply'].get('action')=='rename-confirm');assert state()['reply'].get('error') and not (docs/'UPPER-one.txt').exists() and (docs/'UPPER-two.txt').read_text()=='bystander';checks.append('Destination created after actual preview cannot be replaced or partially rename earlier rows')
 log=(out/'shell.log').read_text();assert not any(marker in log for marker in ('ReferenceError:','TypeError:','Binding loop','Failed to load configuration','Cannot assign','Accessible attached property must')),log
 (out/'result.json').write_text(json.dumps(dict(passed=True,checks=checks,source=call('git','-C',str(repo),'rev-parse','HEAD'),launcher=str(a.launcher),sway=a.sway,effects=a.effects,confirmationLatencySeconds=confirmationLatency,fixtureSha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),assetsSha256={str(path.relative_to(assets)):hashlib.sha256(path.read_bytes()).hexdigest() for path in sorted(assets.rglob('*')) if path.is_file()}),indent=2))
finally:
 for process in reversed(processes):
  if process.poll() is None:os.killpg(process.pid,signal.SIGTERM)
 for process in reversed(processes):
  try:process.wait(timeout=5)
  except sp.TimeoutExpired:os.killpg(process.pid,signal.SIGKILL);process.wait(timeout=5)
 close_reaped_pipes(processes)

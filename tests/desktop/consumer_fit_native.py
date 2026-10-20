"""Private production Calendar/Files screen-identity fit and state preservation."""
import argparse,hashlib,json,os,re,shutil,signal,subprocess as sp,sys,tempfile,time
from pathlib import Path
from fixture_resources import close_reaped_pipes
from private_native_bus import start_private_bus
p=argparse.ArgumentParser();p.add_argument('--launcher',type=Path,required=True);p.add_argument('--sway',required=True);p.add_argument('--effects',action='store_true');p.add_argument('--native-frame',choices=['on','off'],required=True);p.add_argument('--output',type=Path,required=True);a=p.parse_args()
repo=Path(__file__).resolve().parents[2];out=a.output.resolve();out.mkdir(parents=True,exist_ok=False)
private=tempfile.TemporaryDirectory(prefix='lumen-consumer-fit-');processes=[]
env=dict(os.environ,HOME=private.name,XDG_RUNTIME_DIR=private.name)
for key in ('CONFIG','STATE','CACHE','DATA'):env['XDG_'+key+'_HOME']=str(Path(private.name)/key.lower())
for key in ('DISPLAY','WAYLAND_DISPLAY','SWAYSOCK','DBUS_SESSION_BUS_ADDRESS','AT_SPI_BUS_ADDRESS'):env.pop(key,None)
prefix=re.split(r'\n[ \t]*exec quickshell ',a.launcher.read_text(),maxsplit=1)[0]
env.update(dict(value.decode().split('=',1) for value in sp.check_output(['bash','-c',prefix+'\nexec env -0'],env=env).split(b'\0') if b'=' in value))
env.update(WLR_BACKENDS='headless',WLR_HEADLESS_OUTPUTS='2',WLR_RENDERER='gles2' if a.effects else 'pixman',WLR_RENDERER_FORCE_SOFTWARE='1' if a.effects else '0',WLR_RENDERER_ALLOW_SOFTWARE='1',LIBGL_ALWAYS_SOFTWARE='1',WLR_LIBINPUT_NO_DEVICES='1',QT_QPA_PLATFORM='wayland',QT_QPA_PLATFORMTHEME='generic',QT_QUICK_BACKEND='software',QT_ACCESSIBILITY='0',NO_AT_BRIDGE='1',GTK_A11Y='none',LUMEN_FILE_SERVICE='0',LUMEN_LAUNCH_SCOPES='0',LUMEN_NATIVE_WINDOW_ACTIONS='0',LUMEN_NATIVE_WINDOW_CONTROLS='1',LUMEN_UNIFIED_OWNED_HEADERS='0' if a.native_frame=='on' else '1',GLOBAL_MENU_PYTHON=sys.executable)
assets=out/'assets';shutil.copytree(repo/'res/lumen-desktop',assets)
# Disable the unused host-volume discovery branch in the private backend copy.
backend=assets/'shell/files.py';original=backend.read_text();needle="    if action=='volumes':\n";assert original.count(needle)==1;backend.write_text(original.replace(needle,needle+"        return dict(volumes=[])\n"))
palettes=json.loads((assets/'appearance/palette.json').read_text());appearance=out/'appearance.json';appearance.write_text(json.dumps(dict(font=palettes['font'],palette=palettes['dark'],preferences=dict(reduceMotion=True,reduceTransparency=True))));env['LUMEN_APPEARANCE_STATE']=str(appearance)
folder=Path(private.name)/'Synthetic';folder.mkdir();(folder/'selected.txt').write_text('Synthetic preview preserved across display changes.\n')
binpath=out/'bin';binpath.mkdir();provider=binpath/'lumen-calendar';provider.write_text('#!'+sys.executable+'\nimport json,sys\nr=json.loads(sys.stdin.readline());assert r["action"] in ("list","load")\nprint(json.dumps(dict(configured=False,providers=[],events=[],days=[],total=0)))\n');provider.chmod(0o755);env['PATH']=str(binpath)+':'+env['PATH']
qml=out/'shell.qml';qml.write_text('''import QtQuick
import QtQuick.Window
import Quickshell
import Quickshell.Io
import "assets/shell" as D
ShellRoot {id:fixture
 QtObject {id:exported;property var state:({appName:"",title:"",windowId:0})}
 QtObject {id:stub;property int focusEpoch:0;property var windows:[];property var exportedMenu:exported;property date currentDate:new Date(2026,9,20);function activeFiles(window){}function focusWindow(window){}function claimPopup(name){}function focusOwnedWindow(name){}function openSettings(page){}}
 D.FilesState {id:service;path:'''+json.dumps(str(folder))+''';viewMode:"list";persistViewPreferences:true;preferencesLoaded:true}
 D.FilesWindow {id:files;shell:stub;service:service}
 D.CalendarWindow {id:calendar;shell:stub}
 function visit(item,fn){if(fn(item))return item;for(const child of item.children || []){const found=visit(child,fn);if(found)return found;}return null;}
 function facts(window){const fit=visit(window.contentItem,item=>item.ownerWindow===window && item.completedRequests!==undefined);return {visible:window.visible,title:window.title,width:window.width,height:window.height,screen:window.screen?window.screen.name:"",fit:fit?{completed:fit.completedRequests,busy:fit.busy,error:fit.error,result:fit.result}:null};}
 IpcHandler {target:"consumerFit"
  function showConsumer(name:string):void {files.visible=name==="Files";calendar.visible=name==="Calendar";if(name==="Calendar")calendar.open();else service.navigate(service.path,0);}
  function select():void {files.choose(0,0);files.previewOpen=true;files.schedulePreview();}
  function repeat(name:string):void {const window=name==="Files"?files:calendar;for(let i=0;i<5;i++)window.fitScreen();}
  function state(name:string):string {return JSON.stringify({window:fixture.facts(name==="Files"?files:calendar),ready:service.ready && !service.busy && service.rows.length===1,selection:files.selectedPaths,previewOpen:files.previewOpen,preview:service.preview,previewBusy:service.previewBusy,path:service.path,error:service.error});}
 }
}''')
config=out/'sway.conf';config.write_text('font sans 10\noutput HEADLESS-1 resolution 1920x1440 position 0 0 scale 1\noutput HEADLESS-2 resolution 1000x700 position 1920 0 scale 1.25\noutput * bg #29445b solid_color\nfor_window [all] floating enable\ndefault_floating_border '+('normal 1' if a.native_frame=='on' else 'pixel 0')+'\n')
def launch(argv,name):
 with (out/name).open('w') as log:process=sp.Popen(argv,env=env,stdout=log,stderr=sp.STDOUT,start_new_session=True)
 processes.append(process);return process
def call(*argv):return sp.check_output(argv,env=env,text=True,timeout=10).strip()
def ipc(command):
 result=json.loads(call('swaymsg',command));assert all(row.get('success') for row in result),result
def invoke(method,*args):return call('quickshell','-p',str(qml),'ipc','call','consumerFit',method,*args)
def state(name):return json.loads(invoke('state',name))
def nodes(node):
 yield node
 for child in node.get('nodes',[])+node.get('floating_nodes',[]):yield from nodes(child)
def window(title):return next((node for node in nodes(json.loads(call('swaymsg','-r','-t','get_tree'))) if node.get('app_id')=='org.quickshell' and node.get('name')==title),None)
def wait(fn):
 deadline=time.monotonic()+15
 while time.monotonic()<deadline:
  try:
   value=fn()
   if value:return value
  except (OSError,ValueError,sp.SubprocessError):pass
  time.sleep(.05)
 (out/'failed-state.json').write_text(json.dumps(dict(consumers={name:state(name) for name in ('Files','Calendar')},tree=json.loads(call('swaymsg','-r','-t','get_tree'))),indent=2))
 raise AssertionError('Consumer fit bounded condition failed')
records=[]
try:
 start_private_bus(env,out,processes);launch([a.sway,'-d','--unsupported-gpu','-c',str(config)],'compositor.log');wait(lambda:list(Path(private.name).glob('sway-ipc.*.sock')));env['SWAYSOCK']=str(next(Path(private.name).glob('sway-ipc.*.sock')));env['WAYLAND_DISPLAY']=next(path.name for path in Path(private.name).glob('wayland-*') if not path.name.endswith('.lock'));ipc('lumen_window margins disable');ipc('focus output HEADLESS-1');launch(['quickshell','-p',str(qml)],'shell.log')
 wait(lambda:state('Files').get('window'))
 for name in ('Files','Calendar'):
  invoke('showConsumer',name);wait(lambda:state(name)['window']['visible']);title=state(name)['window']['title'];node=wait(lambda:window(title));identity=node['id'];wait(lambda:state(name)['window']['fit']['completed']>0 and not state(name)['window']['fit']['busy'])
  if name=='Files':
   wait(lambda:state(name)['ready']);invoke('select');wait(lambda:state(name)['selection']==[str(folder/'selected.txt')] and not state(name)['previewBusy'] and bool(state(name)['preview']))
  before=state(name);completed=before['window']['fit']['completed']
  ipc(f'[con_id={identity}] move container to output HEADLESS-2')
  wait(lambda:state(name)['window']['screen']=='HEADLESS-2' and state(name)['window']['fit']['completed']>completed and not state(name)['window']['fit']['busy'])
  small=state(name);outcome=small['window']['fit']['result'];assert not small['window']['fit']['error'],small
  wait(lambda:state(name)['window']['width']==outcome['clientWidth'] and state(name)['window']['height']==outcome['clientHeight'])
  node=window(title);assert node['id']==identity
  assert outcome['outerWidth']<=768 and outcome['outerHeight']<=(460 if name=='Calendar' else 410),outcome
  small=state(name);rect=node['rect'];completed=small['window']['fit']['completed'];invoke('repeat',name)
  wait(lambda:state(name)['window']['fit']['completed']>completed and not state(name)['window']['fit']['busy']);repeat=state(name);assert repeat['window']['fit']['completed']==completed+1 and not repeat['window']['fit']['result']['changed'] and window(title)['rect']==rect,repeat
  call('grim','-o','HEADLESS-2','-s','1.25',str(out/(name.lower()+'-small.png')))
  completed=repeat['window']['fit']['completed'];ipc(f'[con_id={identity}] move container to output HEADLESS-1');wait(lambda:state(name)['window']['screen']=='HEADLESS-1' and state(name)['window']['fit']['completed']>completed and not state(name)['window']['fit']['busy']);returned=state(name);assert not returned['window']['fit']['error'] and not returned['window']['fit']['result']['changed'],returned
  assert (returned['window']['width'],returned['window']['height'])==(small['window']['width'],small['window']['height']),returned
  if name=='Files':assert returned['selection']==before['selection'] and returned['previewOpen']==before['previewOpen'] and returned['preview']==before['preview'],(before,returned)
  records.append(dict(consumer=name,before=before,small=small,repeat=repeat,returned=returned,tree=node));ipc('focus output HEADLESS-1')
 (out/'result.json').write_text(json.dumps(dict(passed=True,cases=records,source=call('git','-C',str(repo),'rev-parse','HEAD'),launcher=str(a.launcher),compositor=a.sway,effects=a.effects,nativeFrame=a.native_frame,fixtureSha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),providerSha256=hashlib.sha256(provider.read_bytes()).hexdigest(),assetsSha256={str(path.relative_to(assets)):hashlib.sha256(path.read_bytes()).hexdigest() for path in sorted(assets.rglob('*')) if path.is_file()},originalFilesBackendSha256=hashlib.sha256(original.encode()).hexdigest(),privateFilesBackendSha256=hashlib.sha256(backend.read_bytes()).hexdigest()),indent=2))
finally:
 for process in reversed(processes):
  if process.poll() is None:
   os.killpg(process.pid,signal.SIGTERM)
   try:process.wait(timeout=5)
   except sp.TimeoutExpired:os.killpg(process.pid,signal.SIGKILL);process.wait(timeout=5)
 close_reaped_pipes(processes);private.cleanup()

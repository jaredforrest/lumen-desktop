"""Private actual stack actions and captured owned directory handoff."""
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
 args=[str(runtime),'/source/tests/desktop/folder_stack_native.py','--inside','--launcher',str(a.launcher),'--sway',str(a.sway),'--pointer',str(a.pointer),'--font-config',str(a.font_config),'--source-revision',revision,'--parent-net',os.readlink('/proc/self/ns/net')]
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
with tempfile.TemporaryDirectory(prefix='lumen-folder-stack-') as temporary:
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
  raise AssertionError('Folder-stack observation deadline')
 def nodes(node):
  yield node
  for child in node.get('nodes',[])+node.get('floating_nodes',[]):yield from nodes(child)



 def ui():return json.loads(call('quickshell','-p',qml,'ipc','call','stackAudit','status'))
 def ipc(action,*args):return call('quickshell','-p',qml,'ipc','call','stackAudit',action,*args)
 def point(path):
  box=json.loads(ipc('point',str(path)));assert box and box['width']>0 and box['height']>0,box
  checked('seat seat0 cursor set %d %d'%(box['x']+box['width']/2,box['y']+box['height']/2));pointer.stdin.write('m 0 0\n');pointer.stdin.flush();wait(lambda:json.loads(ipc('point',str(path)))['hovered'])
 def press(path):point(path);pointer.stdin.write('p\n');pointer.stdin.flush();wait(lambda:json.loads(ipc('point',str(path)))['down'])
 def release():pointer.stdin.write('r\n');pointer.stdin.flush()
 def records():return [json.loads(line) for line in opened.read_text().splitlines()] if opened.exists() else []
 def show(folder,presentation='list'):
  ipc('presentFolder',str(folder),presentation);return wait(lambda:(v if (v:=ui())['visible'] and not v['listing'] and v['folder']==str(folder) and v['facts'] else None))
 checks=[]
 try:
  start_private_bus(env,a.output,processes)
  config=private/'sway.conf';config.write_text('output HEADLESS-1 resolution 1280x900 position 0 0\noutput HEADLESS-2 disable\nseat seat0 fallback true\nxwayland disable\nfocus_follows_mouse no\nfor_window [all] floating enable, border normal 1\n')
  launch([str(a.sway),'--unsupported-gpu','-c',str(config)],'sway.log');env['SWAYSOCK']=str(wait(lambda:next(Path(env['XDG_RUNTIME_DIR']).glob('sway-ipc*.sock'),None)));env['WAYLAND_DISPLAY']=wait(lambda:next((x.name for x in Path(env['XDG_RUNTIME_DIR']).glob('wayland-*') if not x.name.endswith('.lock')),None))
  pointer=launch([str(a.pointer),'pointer','move','0','0'],'pointer.log',stdin=sp.PIPE,text=True);pointer.stdin.write('m 0 0\n');pointer.stdin.flush();launch(['wtype','-s','120000'],'keyboard.log');wait(lambda:any(seat.get('capabilities',0)&1 for seat in sway('-t','get_seats')))
  env.update(LUMEN_LAUNCH_SCOPES='0',XDG_CURRENT_DESKTOP='GNOME',DBUS_SYSTEM_BUS_ADDRESS='unix:path='+str(private/'absent-system'),PULSE_SERVER='unix:'+str(private/'absent-audio'))
  env['XDG_DATA_DIRS']=env['XDG_DATA_HOME'];apps=Path(env['XDG_DATA_HOME'])/'applications';apps.mkdir();opened=a.output/'opened.jsonl';observer=private/'open-observer.py'
  observer.write_text('import hashlib,json,sys\nfrom pathlib import Path\nfrom urllib.parse import unquote,urlsplit\nuri=sys.argv[2];path=Path(unquote(urlsplit(uri).path)) if uri.startswith("file:") else Path(uri)\nwith Path(sys.argv[1]).open("a") as out:out.write(json.dumps({"uri":uri,"path":str(path),"sha256":hashlib.sha256(path.read_bytes()).hexdigest()})+"\\n")\n')
  (apps/'lumen-stack-observer.desktop').write_text('[Desktop Entry]\nType=Application\nName=Stack Observer\nExec='+sys.executable+' '+str(observer)+' '+str(opened)+' %U\nMimeType=text/plain;\nTerminal=false\n')
  (Path(env['XDG_CONFIG_HOME'])/'mimeapps.list').write_text('[Default Applications]\ntext/plain=lumen-stack-observer.desktop;\n')
  docs=Path(env['HOME'])/'Mixed';docs.mkdir();alpha=docs/'Alpha # ü.txt';alpha.write_text('captured alpha');beta=docs/'Beta.txt';beta.write_text('captured beta');child=docs/'Child';child.mkdir();(child/'inside.txt').write_text('owned child');link=docs/'Link.txt';link.symlink_to(alpha);(docs/'picture.png').write_bytes(b'only synthetic icon metadata');empty=Path(env['HOME'])/'Empty';empty.mkdir();huge=Path(env['HOME'])/'Huge';huge.mkdir()
  for index in range(5005):(huge/('item-%05d.txt'%index)).write_text(str(index))
  assets=private/'assets';shutil.copytree(repo/'res/lumen-desktop',assets)
  # Read-only geometry and exact owned helper signal access; no provider/action replacement.
  target=assets/'shell/FolderStack.qml';original=target.read_text();needle='    id:root';assert original.count(needle)==1;modified=original.replace(needle,needle+'\n    property alias fixtureListing:worker\n    property alias fixtureAction:actionWorker\n    property bool fixtureHoldListing:false',1);input_gate='onStarted:{received=false;write(JSON.stringify(request)+"\\n");}';assert modified.count(input_gate)==2;modified=modified.replace(input_gate,'onStarted:{received=false;if(!root.fixtureHoldListing)write(JSON.stringify(request)+"\\n");}',1);target.chmod(0o600);target.write_text(modified)
  import hashlib
  (a.output/'instrumentation.json').write_text(json.dumps(dict(originalSha256=hashlib.sha256(original.encode()).hexdigest(),modifiedSha256=hashlib.sha256(modified.encode()).hexdigest(),roundtrip=modified.replace('\n    property alias fixtureListing:worker\n    property alias fixtureAction:actionWorker\n    property bool fixtureHoldListing:false','',1).replace('onStarted:{received=false;if(!root.fixtureHoldListing)write(JSON.stringify(request)+"\\n");}',input_gate,1)==original,scope='Exact child Process aliases and initial listing stdin gate only; real provider, action validator, row, directory, MIME dispatch and UI unchanged.'),indent=2))
  palette=json.loads((assets/'appearance/palette.json').read_text());appearance=private/'appearance.json';appearance.write_text(json.dumps(dict(font=palette['font'],palette=palette['dark'],preferences=dict(reduceMotion=True))));env['LUMEN_APPEARANCE_STATE']=str(appearance)
  qml=private/'stack.qml';qml.write_text('''import QtQuick
import QtQuick.Window
import Quickshell
import Quickshell.Io
import Quickshell.Wayland
import "assets/shell" as D
ShellRoot {id:fixture
 property string popupOwner:""
 property int focusEpoch:0
 property var windows:ToplevelManager.toplevels.values
 property var ownedContext:null
 property var screenSaverPreferences:null
 property var heldDirectory:null
 property bool holdDirectory:false
 property string reportedError:""
 property var persistedOptions:[]
 property var exportedMenu:exported
 signal dismissPopups(string owner)
 function claimPopup(owner){popupOwner=owner;dismissPopups(owner);}
 function updateFolderStack(path,options){persistedOptions=persistedOptions.concat([{path:path,options:options}]);}
 function reportApplicationError(message){reportedError=message;}
 function focusWindow(window){window.activate();}
 function focusOwnedWindow(title){}
 function activeFiles(window){}
 function newFilesWindow(path){throw Error("Unexpected new window");}
 function openFiles(path,facts){if(holdDirectory)heldDirectory={path:path,facts:facts};else files.openPath(path,facts);}
 QtObject {id:exported;property var state:({appName:"",title:"",windowId:0})}
 D.FilesState {id:service;persistViewPreferences:false}
 D.FilesWindow {id:files;shell:fixture;service:service;visible:false;previewOpen:false}
 D.FolderStack {id:stack;shell:fixture}
 function find(path,item){if(item.visible && ((path==="@footer" && item.objectName==="folder-stack-open-in-files") || (path==="@sort" && item.objectName==="folder-stack-sort") || (path==="@presentation" && item.objectName==="folder-stack-presentation") || (path==="@grid" && item.objectName==="folder-stack-grid") || (item.modelData && item.modelData.path===path && ["folder-stack-list-row","folder-stack-grid-row"].includes(item.objectName))))return item;for(const child of item.children || []){const got=find(path,child);if(got)return got;}return null;}
 IpcHandler {target:"stackAudit"
 function presentFolder(path:string,presentation:string):void{stack.show(path,"HEADLESS-1",{sort:"name",descending:false,presentation:presentation});}
 function reload():void{stack.reload();}
 function closeFiles():void{files.visible=false;}
 function focusControl(path:string):bool{const item=fixture.find(path,stack.contentItem);if(!item)return false;item.forceActiveFocus();return true;}
 function focusFooter():bool{const item=fixture.find("@footer",stack.contentItem);if(!item)return false;item.forceActiveFocus();return true;}
 function hold(value:bool):void{fixture.holdDirectory=value;fixture.heldDirectory=null;}
 function releaseDirectory():void{const captured=fixture.heldDirectory;fixture.heldDirectory=null;if(captured)files.openPath(captured.path,captured.facts);}
 function choose(index:int):void{stack.selected=index;}
 function holdListing(value:bool):void{stack.fixtureHoldListing=value;}
 function killListing():bool{if(!stack.fixtureListing.running || stack.fixtureListing.received)return false;stack.fixtureListing.signal(15);return true;}
 function status():string{return JSON.stringify({visible:stack.visible,folder:stack.folder,rowCount:stack.rows.length,rows:stack.rows.slice(0,30).map(r=>({path:r.path,name:r.name,directory:r.directory,facts:r.actionFacts})),facts:stack.folderFacts,error:stack.error,busy:stack.actionBusy,listing:stack.fixtureListing.running,generation:stack.generation,sortPopup:fixture.find("@sort",stack.contentItem)?fixture.find("@sort",stack.contentItem).popup.visible:false,scrollY:fixture.find("@grid",stack.contentItem)?fixture.find("@grid",stack.contentItem).contentY:0,sort:stack.sort,presentation:stack.presentation,persisted:fixture.persistedOptions,grid:stack.grid,limited:stack.limited,total:stack.total,held:fixture.heldDirectory,filesVisible:files.visible,filesBusy:service.busy,filesPath:service.path,filesRows:service.rows.map(r=>r.name),filesError:service.error,filesHistory:service.history.length,reported:fixture.reportedError});}
 function point(path:string):string{const item=fixture.find(path,stack.contentItem);if(!item)return "null";const at=item.mapToItem(stack.contentItem,0,0);return JSON.stringify({x:at.x,y:at.y,width:item.width,height:item.height,hovered:item.hovered,down:item.down,active:item.activeFocus});}
 }
}''')
  launch(['quickshell','-p',str(qml)],'shell.log');wait(lambda:ui()['generation']>=0)
  state=show(empty);assert not state['rows'] and not state['error'];assert ipc('focusFooter')=='true';wait(lambda:json.loads(ipc('point','@footer'))['active']);call('wtype','-k','space');wait(lambda:ui()['filesVisible'] and not ui()['filesBusy'] and ui()['filesPath']==str(empty));assert ui()['filesRows']==[];checks.append('Actual empty stack keyboard footer opens production Files with captured directory facts')
  ipc('closeFiles');state=show(docs);assert {r['name'] for r in state['rows']}=={p.name for p in docs.iterdir()};assert all(len(r['facts'])==6 and all(isinstance(v,str) for v in r['facts']) for r in state['rows']);checks.append('Actual mixed stack publishes lossless captured source versions and list presentation')
  assert ipc('focusControl','@sort')=='true';wait(lambda:json.loads(ipc('point','@sort'))['active']);call('wtype','-k','Down');wait(lambda:ui()['sort']=='modified' and not ui()['listing']);assert ui()['persisted'][-1]['path']==str(docs);checks.append('Actual keyboard sort control persists only its captured folder invocation')
  assert ipc('focusControl','@presentation')=='true';wait(lambda:json.loads(ipc('point','@presentation'))['active']);call('wtype','-k','Down');wait(lambda:ui()['presentation']=='grid');assert ui()['persisted'][-1]['path']==str(docs);call('wtype','-k','Up');wait(lambda:ui()['presentation']=='list');checks.append('Actual keyboard presentation control switches list/grid without changing folder ownership')
  before=len(ui()['persisted']);press('@sort');release();wait(lambda:ui()['sortPopup']);show(empty);assert not ui()['sortPopup'] and len(ui()['persisted'])==before;show(docs);checks.append('Actual open sort popup retires on a different folder invocation without persisting to the new folder')
  press(alpha);release();wait(lambda:len(records())==1 and not ui()['visible']);assert records()[-1]['path']==str(alpha) and records()[-1]['sha256']==hashlib.sha256(alpha.read_bytes()).hexdigest();checks.append('Physical file click reaches actual private default handler with exact literal URI and original bytes')
  show(docs);press(alpha);old=private/'old-alpha';alpha.rename(old);alpha.write_text('replacement alpha');release();wait(lambda:ui()['error'] and not ui()['busy']);assert len(records())==1 and ui()['visible'];checks.append('Actual filesystem replacement during held press refuses external dispatch and exposes error')
  state=show(docs);press(alpha);ipc('reload');wait(lambda:not ui()['listing'] and ui()['facts']);release();wait(lambda:not ui()['busy']);assert len(records())==1;checks.append('Reload while pointer held cannot transfer activation to refreshed row')
  state=show(docs);press(alpha);call('wtype','-k','Down');release();wait(lambda:len(records())==2 and not ui()['busy']);assert records()[-1]['path']==str(alpha) and records()[-1]['sha256']==hashlib.sha256(alpha.read_bytes()).hexdigest();checks.append('Pointer press owns its file despite keyboard selection moving before release')
  show(docs);press(link);release();wait(lambda:ui()['error'] and not ui()['busy']);assert len(records())==2 and link.is_symlink();checks.append('Symbolic-link stack action is explicitly unsupported without dereference or deletion')
  state=show(docs);ipc('hold','true');press(child);release();wait(lambda:ui()['held'] is not None);assert ui()['held']['facts']==next(r['facts'] for r in state['rows'] if r['path']==str(child));retired=private/'retired-child';child.rename(retired);child.mkdir();(child/'replacement.txt').write_text('replacement');previous=ui()['filesPath'];history=ui()['filesHistory'];ipc('releaseDirectory');wait(lambda:ui()['filesVisible'] and not ui()['filesBusy'] and ui()['filesError']);assert ui()['filesPath']==previous and ui()['filesHistory']==history and 'replacement.txt' not in ui()['filesRows'];checks.append('Delayed owned directory handoff refuses replaced target without committing Files rows/path/history')
  ipc('closeFiles');ipc('hold','false');show(docs);press(child);release();wait(lambda:ui()['filesVisible'] and not ui()['filesBusy'] and ui()['filesPath']==str(child));assert ui()['filesRows']==['replacement.txt'];checks.append('Explicit fresh directory selection opens production Files through anchored initial listing')
  ipc('closeFiles');state=show(huge,'grid');assert state['grid'] and state['limited'] and state['rowCount']==5000 and state['total']==5000;box=json.loads(ipc('point','@footer'));assert 0<=box['y'] and box['y']+box['height']<=900;box=json.loads(ipc('point','@grid'));checked('seat seat0 cursor set %d %d'%(box['x']+box['width']/2,box['y']+box['height']/2));pointer.stdin.write('m 0 0\nw 5\n');pointer.stdin.flush();wait(lambda:ui()['scrollY']>0);call('grim',a.output/'huge-stack.png');checks.append('Large grid is explicitly limited and scrollable with a reachable folder action inside output bounds')
  ipc('holdListing','true');ipc('presentFolder',str(huge),'list');wait(lambda:ui()['listing']);assert ipc('killListing')=='true';wait(lambda:ui()['error'] and not ui()['listing']);assert not ui()['rows'] and not ui()['facts'];ipc('holdListing','false');ipc('reload');wait(lambda:not ui()['listing'] and bool(ui()['facts']) and ui()['rowCount']==5000);checks.append('Actual listing child death reports unavailable, then explicit reload restores anchored rows')
  (a.output/'result.json').write_text(json.dumps(dict(passed=True,checks=checks,source=a.source_revision,compositor=str(a.sway),dispatches=records(),liveSocketUnchanged=os.environ.get('SWAYSOCK')==original_socket),indent=2));print(json.dumps(dict(passed=True,checks=len(checks))))
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

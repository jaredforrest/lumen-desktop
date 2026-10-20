"""Actual private Quick Look/Files controls and captured action dispatch observers."""
import argparse,hashlib,json,os,re,shutil,signal,subprocess as sp,sys,tempfile,time
from pathlib import Path
from PIL import Image
from private_native_bus import start_private_bus
from fixture_resources import close_reaped_pipes
p=argparse.ArgumentParser();p.add_argument('--launcher',type=Path,required=True);p.add_argument('--sway',required=True);p.add_argument('--pointer-tool',required=True);p.add_argument('--output',type=Path,required=True);p.add_argument('--effects',action='store_true');a=p.parse_args();out=a.output.resolve();out.mkdir(parents=True,exist_ok=False);processes=[];checks=[]
with tempfile.TemporaryDirectory(prefix='lumen-preview-actions-') as temporary:
 home=Path(temporary);env=dict(os.environ,HOME=temporary,XDG_RUNTIME_DIR=temporary)
 for key in ('CONFIG','CACHE','DATA','STATE'):env['XDG_'+key+'_HOME']=str(home/key.lower())
 for key in ('WAYLAND_DISPLAY','SWAYSOCK','DISPLAY','DBUS_SESSION_BUS_ADDRESS','AT_SPI_BUS_ADDRESS'):env.pop(key,None)
 prefix=re.split(r'\n[ \t]*exec quickshell ',a.launcher.read_text(),maxsplit=1)[0];env.update(dict(v.decode().split('=',1) for v in sp.check_output(['bash','-c',prefix+'\nexec env -0'],env=env).split(b'\0') if b'=' in v))
 env.update(QT_QPA_PLATFORM='wayland',QT_QPA_PLATFORMTHEME='generic',QT_QUICK_BACKEND='software',QT_ACCESSIBILITY='0',NO_AT_BRIDGE='1',GTK_A11Y='none',WLR_BACKENDS='headless',WLR_RENDERER='gles2' if a.effects else 'pixman',WLR_RENDERER_ALLOW_SOFTWARE='1',WLR_RENDERER_FORCE_SOFTWARE='1' if a.effects else '0',LIBGL_ALWAYS_SOFTWARE='1',WLR_LIBINPUT_NO_DEVICES='1',LUMEN_LAUNCH_SCOPES='0',GLOBAL_MENU_ENABLED='0',LUMEN_FILE_SERVICE='0')
 env['PREVIEW_ACTION_AUDIT']=str(out);bin=home/'bin';bin.mkdir();env['PATH']=str(bin)+':'+env['PATH'];docs=home/'Documents';docs.mkdir();one=docs/'one.png';two=docs/'two.png';Image.new('RGB',(32,32),'red').save(one);Image.new('RGB',(32,32),'blue').save(two)
 observer=bin/'xdg-open';observer.write_text('#!'+sys.executable+'\nimport json,os,sys\nfrom pathlib import Path\nwith (Path(os.environ["PREVIEW_ACTION_AUDIT"])/"opened.jsonl").open("a") as stream:stream.write(json.dumps(sys.argv[1:])+"\\n")\n');observer.chmod(0o700)
 appdir=home/'data/applications';appdir.mkdir(parents=True);desktop=appdir/'lumen-private-observer.desktop';desktop.write_text('[Desktop Entry]\nType=Application\nName=Private Preview Observer\nExec='+str(observer)+' %U\nMimeType=image/png;application/octet-stream;\n');(appdir/'mimeinfo.cache').write_text('[MIME Cache]\nimage/png=lumen-private-observer.desktop;\napplication/octet-stream=lumen-private-observer.desktop;\n')
 assets=out/'assets';shutil.copytree(Path(env['LUMEN_NOTIFICATION_HELPER']).parent.parent,assets);tools=assets/'shell/context_tools.py';original=tools.read_text();adapter='''def private_preview_observer(data):
    import hashlib
    source=Path(data['path']);value=source.read_bytes();out=Path(os.environ['PREVIEW_ACTION_AUDIT'])
    with (out/'content.jsonl').open('a') as stream:stream.write(json.dumps(dict(action=data['action'],path=str(source),sha256=hashlib.sha256(value).hexdigest(),pid=os.getpid()))+'\\n')
    if (out/'hold-content').exists():
        import time
        while True:time.sleep(1)
    if data['action']=='ocr':return dict(text='Private recognized text',message='Private observer')
    target=out/('subject-'+str(os.getpid())+'.png');target.write_bytes(value)
    return dict(subject=str(target),message='Private extraction observer')

'''
 replacements={"if action=='cutout':return cutout(data['path'])":"if action=='cutout':return private_preview_observer(data)","if action=='ocr':return ocr(data['path'],data.get('page',1),data.get('seconds'))":"if action=='ocr':return private_preview_observer(data)"};modified=original
 for old,new in replacements.items():assert modified.count(old)==1;modified=modified.replace(old,new)
 modified=modified.replace('def handle(data):',adapter+'def handle(data):',1);roundtrip=modified.replace(adapter,'',1)
 for old,new in replacements.items():roundtrip=roundtrip.replace(new,old)
 assert roundtrip==original;tools.chmod(0o600);tools.write_text(modified);(out/'instrumentation.json').write_text(json.dumps(dict(originalSha256=hashlib.sha256(original.encode()).hexdigest(),modifiedSha256=hashlib.sha256(modified.encode()).hexdigest(),roundtrip=True,scope='Only fixed OCR/extraction helper dispatch replaced by private byte-reading observers; UI, broker, source authority and child supervisor unchanged.'),indent=2))
 quick_source=assets/'shell/QuickLookWindow.qml';quick_original=quick_source.read_text();quick_marker='    id:root\n';quick_added=quick_marker+'    readonly property var fixtureMenu:previewActions\n';assert quick_original.count(quick_marker)==1;quick_modified=quick_original.replace(quick_marker,quick_added,1);assert quick_modified.replace(quick_added,quick_marker,1)==quick_original;quick_source.chmod(0o600);quick_source.write_text(quick_modified)
 manifest=json.loads((out/'instrumentation.json').read_text());manifest.update(quickOriginalSha256=hashlib.sha256(quick_original.encode()).hexdigest(),quickModifiedSha256=hashlib.sha256(quick_modified.encode()).hexdigest(),quickScope='Only read-only actual Menu object exposure for native popup item geometry/status');(out/'instrumentation.json').write_text(json.dumps(manifest,indent=2))
 qml=out/'shell.qml';qml.write_text('''import QtQuick
import QtQuick.Window
import Quickshell
import Quickshell.Io
import "assets/shell" as D
ShellRoot {
 id:root
 property var fileErrors:[]
 Connections {target:service;function onErrorChanged(){if(service.error)root.fileErrors=root.fileErrors.concat([service.error]);}}
 QtObject {id:exported;property var state:({appName:"",title:"",windowId:0})}
 QtObject {id:stub;property int focusEpoch:0;property var windows:[];property var exportedMenu:exported;property var quickLookWindow:quick;function activeFiles(w){}function focusWindow(w){}function focusOwnedWindow(title){}function claimPopup(owner){}}
 D.FilesState {id:service;persistViewPreferences:false;path:'''+json.dumps(str(docs))+'''}
 D.FilesWindow {id:files;shell:stub;service:service;visible:true}
 D.QuickLookWindow {id:quick}
 function find(item,text,seen){if(!item || seen.includes(item))return null;seen.push(item);if((item.text===text || (text==="Private Preview Observer" && item.text===text+" (default)")) && item.visible && item.enabled && item.Window.window && item.Window.window.visible)return item;for(const child of [...(item.children || []),...(item.data || []),...(item.contentData || [])]){const found=find(child,text,seen);if(found)return found;}if(item.contentItem && item.contentItem!==item)return find(item.contentItem,text,seen);return null;}
 function controller(item){if(item.objectName==="owned-preview-state")return item;for(const child of item.children || []){const found=controller(child);if(found)return found;}return null;}
 IpcHandler {target:"fixture"
  function startPreview(path:string):void{quick.show(path,files);}
  function closePreview():void{quick.visible=false;}
  function choose(path:string):void{files.choose(service.rows.findIndex(row=>row.path===path),0);}
  function refresh():void{service.refresh();}
  function applications():void{service.act("applications",{path:files.current.path});}
  function status():string{const c=root.controller(quick.contentItem);return JSON.stringify({result:quick.result,message:quick.actionMessage,filesError:service.error,fileErrors:root.fileErrors,ready:service.ready && !service.busy,current:files.current?files.current.path:null,applications:files.applicationsSelection,actionBusy:c?c.actionBusy:false,retiring:c?c.retiring:false});}
  function button(surface:string,text:string):string{let item=root.find(surface==="quick"?quick.contentItem:files.contentItem,text,[]);if(!item && surface==="quick" && quick.fixtureMenu.visible){for(let i=0;i<quick.fixtureMenu.count;i++){const candidate=quick.fixtureMenu.itemAt(i);if(candidate && candidate.text===text && candidate.enabled)item=candidate;}}if(!item)return "{}";const window=surface==="quick"?quick.contentItem.Window.window:item.Window.window;const point=item.mapToItem(window.contentItem,item.width/2,item.height/2);return JSON.stringify({x:point.x,y:point.y,title:window.title,windowWidth:window.width,windowHeight:window.height,active:window.active || (item.Window.window && item.Window.window.active),activeFocus:item.activeFocus,down:item.down,hovered:item.hovered,pressed:item.pressedSelection});}

 }
 Component.onCompleted:service.navigate(service.path)
}
''')
 def launch(command,name,**kwargs):
  with (out/name).open('w') as log:child=sp.Popen(command,env=env,stdout=log,stderr=sp.STDOUT,start_new_session=True,**kwargs)
  processes.append(child);return child
 def call(*command):return sp.check_output(command,env=env,text=True,timeout=8).strip()
 def wait(fn,label):
  end=time.monotonic()+12
  while time.monotonic()<end:
   try:value=fn()
   except (ValueError,OSError,sp.SubprocessError):value=None
   if value:return value
   time.sleep(.04)
  raise AssertionError('Preview action timeout: '+label)
 def ipc(method,*args):return call('quickshell','-p',str(qml),'ipc','call','fixture',method,*args)
 def state():return json.loads(ipc('status'))
 def records(name):return [json.loads(line) for line in (out/name).read_text().splitlines()] if (out/name).exists() else []
 def leaves(node):
  yield node
  for child in node.get('nodes',[])+node.get('floating_nodes',[]):yield from leaves(child)
 def point(surface,text):
  position=wait(lambda:json.loads(ipc('button',surface,text)),text)
  def owner():return next((node for node in leaves(json.loads(call('swaymsg','-r','-t','get_tree'))) if node.get('name')==position['title'] and node.get('pid')==shell.pid and node.get('app_id')=='org.quickshell'),None)
  node=wait(owner,'mapped exact control owner');window_id=node['id'];call('swaymsg','-r',f'[con_id={window_id}] focus')
  def acknowledged():
   node=owner();current=json.loads(ipc('button',surface,text))
   return (node,current) if node and node['id']==window_id and node.get('focused') and current.get('active') and node.get('window_rect',{}).get('width')==current.get('windowWidth') and node.get('window_rect',{}).get('height')==current.get('windowHeight') else None
  node,position=wait(acknowledged,'focused configured exact control owner')
  (out/'point.json').write_text(json.dumps(dict(position=position,node=node),indent=2));call('swaymsg','-r','seat seat0 cursor set '+str(round(node['rect']['x']+node.get('window_rect',{}).get('x',0)+position['x']))+' '+str(round(node['rect']['y']+node.get('window_rect',{}).get('y',0)+position['y'])));pointer.stdin.write('m 0.5 0.5\n');pointer.stdin.flush();wait(lambda:json.loads(ipc('button',surface,text)).get('hovered'),'hovered exact control');return node
 def press(surface,text):
  node=point(surface,text);pointer.stdin.write('p\n');pointer.stdin.flush();wait(lambda:json.loads(ipc('button',surface,text)).get('down'),'actual pressed '+text)
 def release():pointer.stdin.write('r\n');pointer.stdin.flush()
 def click(surface,text):press(surface,text);release()
 def show(path):ipc('startPreview',str(path));return wait(lambda:(s if (s:=state())['result'].get('sourcePath')==str(path) and s['result'].get('actionCapability') else None),'captured preview')
 def menu(text):click('quick','•••');click('quick',text)
 try:
  start_private_bus(env,out,processes);conf=out/'sway.conf';conf.write_text('output HEADLESS-1 resolution 1280x900\nxwayland disable\nseat seat0 fallback true\nfocus_follows_mouse no\ndefault_border none\nfor_window [app_id="org.quickshell"] floating enable, move position 20 20\n');launch([a.sway,'--unsupported-gpu','-c',str(conf)],'sway.log');wait(lambda:list(home.glob('sway-ipc*.sock')),'compositor');env['SWAYSOCK']=str(next(home.glob('sway-ipc*.sock')));env['WAYLAND_DISPLAY']=next(path.name for path in home.glob('wayland-*') if not path.name.endswith('.lock'));launch(['wtype','-s','600000'],'keyboard.log');pointer=launch([a.pointer_tool,'pointer','move','0','0'],'pointer.log',stdin=sp.PIPE,text=True);shell=launch(['quickshell','-p',str(qml)],'shell.log');wait(lambda:state()['ready'],'Files ready')
  show(one);click('quick','Open');wait(lambda:len(records('opened.jsonl'))==1,'captured Open');assert records('opened.jsonl')[0]==[one.as_uri()];checks.append('Actual Quick Look Open dispatches exact captured source through private observer')
  before=state()['result'];replacement=docs/'replacement.png';Image.new('RGB',(32,32),'green').save(replacement);replacement.replace(one);click('quick','Open');wait(lambda:state()['message']=='The selected file changed','replacement refused');assert len(records('opened.jsonl'))==1 and state()['result']['actionCapability']==before['actionCapability'];checks.append('Original replacement produces zero dispatch and preserves successful immutable preview')
  show(one);press('quick','Open');show(two);release();time.sleep(.15);assert len(records('opened.jsonl'))==1;checks.append('Held pointer Open cannot retarget a newly selected preview')
  menu('Open With…');click('quick','Private Preview Observer');wait(lambda:len(records('opened.jsonl'))==2,'Open With observer');assert records('opened.jsonl')[-1][0] in (str(two),two.as_uri());checks.append('Actual Quick Look Open With chooses registered private application')
  menu('Open With…');wait(lambda:json.loads(ipc('button','quick','Private Preview Observer')),'chooser visible');replacement=docs/'replacement.png';Image.new('RGB',(32,32),'yellow').save(replacement);replacement.replace(two);click('quick','Private Preview Observer');wait(lambda:state()['message']=='The selected file changed','stale chooser refused');assert len(records('opened.jsonl'))==2;checks.append('Stale application chooser cannot dispatch replacement resource')
  show(two);before_hash=hashlib.sha256(two.read_bytes()).hexdigest();menu('Copy Text…');wait(lambda:records('content.jsonl'),'private OCR input');observed=records('content.jsonl')[-1];assert observed['sha256']==before_hash and observed['path']!=str(two) and 'lumen-preview-snapshots-' in observed['path'];call('wtype','-k','Escape');checks.append('Actual OCR menu supplies immutable retained bytes to fixed private helper')
  (out/'hold-content').touch();preserved=state()['result'];menu('Copy Text…');wait(lambda:len(records('content.jsonl'))==2,'held content helper');helper=records('content.jsonl')[-1]['pid'];menu('Cancel Text Recognition');wait(lambda:state()['message']=='Content action canceled' and not state()['actionBusy'],'canceled helper reaped');assert not Path('/proc/'+str(helper)).exists() and state()['result']['actionCapability']==preserved['actionCapability'] and Path(preserved['resourcePath']).exists();(out/'hold-content').unlink();checks.append('Actual Cancel Text Recognition reaps scoped helper while retaining visible immutable preview')
  menu('Lift Subject…');wait(lambda:len(records('content.jsonl'))==3,'private extraction input');assert records('content.jsonl')[-1]['sha256']==before_hash;call('wtype','-k','Escape');checks.append('Actual extraction menu supplies same captured immutable source')
  ipc('closePreview');ipc('choose',str(one));wait(lambda:state()['current']==str(one),'Files selection');ipc('applications');wait(lambda:state()['applications'],'Files captured chooser')
  def owned_descendants(pid):
   children=Path(f'/proc/{pid}/task/{pid}/children').read_text().split()
   for value in children:
    child=int(value);yield child;yield from owned_descendants(child)
  files_pid=next(pid for pid in owned_descendants(shell.pid) if str(assets/'shell/files.py').encode() in Path(f'/proc/{pid}/cmdline').read_bytes());os.kill(files_pid,signal.SIGSTOP)
  try:
   replacement=docs/'replacement.png';Image.new('RGB',(32,32),'purple').save(replacement);replacement.replace(one);click('files','Private Preview Observer')
  finally:os.kill(files_pid,signal.SIGCONT)
  wait(lambda:'The selected file changed' in state()['fileErrors'],'Files stale captured action refused');assert len(records('opened.jsonl'))==2;checks.append('Files chooser rejects replacement after actual queued selection while owned provider paused')
  ipc('refresh');wait(lambda:state()['ready'],'refreshed Files');ipc('choose',str(two));ipc('applications');wait(lambda:state()['applications'],'fresh Files chooser');chooser=state()['applications'];ipc('refresh');wait(lambda:state()['ready'],'unchanged-source background refresh');assert state()['applications']==chooser;click('files','Private Preview Observer');wait(lambda:len(records('opened.jsonl'))==3,'Files Open With observer');assert records('opened.jsonl')[-1][0] in (str(two),two.as_uri());checks.append('Fresh Files chooser dispatches exact registered private observer after stale refusal')
  result=dict(passed=True,checks=checks,launcher=str(a.launcher),sway=a.sway,effects=a.effects,externalOpenBoundary='Predispatch captured identity only; an external handler may later reopen its original path.');(out/'result.json').write_text(json.dumps(result,indent=2));print(json.dumps(result))
 except BaseException:
  try:(out/'failure-button.json').write_text(ipc('button','quick','Open'));(out/'failure-state.json').write_text(json.dumps(state(),indent=2));(out/'failure-tree.json').write_text(call('swaymsg','-r','-t','get_tree'));call('grim','-c','-o','HEADLESS-1',str(out/'failure.png'))
  except (OSError,ValueError,sp.SubprocessError):pass
  raise
 finally:
  for child in reversed(processes):
   if child.poll() is None:child.terminate()
  for child in reversed(processes):
   try:child.wait(timeout=5)
   except sp.TimeoutExpired:child.kill();child.wait(timeout=5)
  close_reaped_pipes(processes)

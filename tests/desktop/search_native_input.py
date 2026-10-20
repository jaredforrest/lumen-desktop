"""Actual launcher input with private real index, delayed transport and launch sink."""
import argparse,hashlib,json,os,re,shutil,signal,struct,subprocess as sp,sys,tempfile,time,zlib
from pathlib import Path
from private_native_bus import start_private_bus
p=argparse.ArgumentParser();p.add_argument('--launcher',type=Path,required=True);p.add_argument('--sway',required=True);p.add_argument('--pointer-tool',required=True);p.add_argument('--effects',action='store_true');p.add_argument('--output',type=Path,required=True);a=p.parse_args();out=a.output.resolve();out.mkdir(parents=True,exist_ok=False);processes=[];checks=[]
with tempfile.TemporaryDirectory(prefix='lumen-search-native-') as temporary:
 home=Path(temporary);env=dict(os.environ,HOME=temporary,XDG_RUNTIME_DIR=temporary)
 for key in ('CONFIG','STATE','CACHE','DATA'):env['XDG_'+key+'_HOME']=str(home/key.lower())
 for key in ('SWAYSOCK','WAYLAND_DISPLAY','DISPLAY','DBUS_SESSION_BUS_ADDRESS','AT_SPI_BUS_ADDRESS'):env.pop(key,None)
 prefix=re.split(r'\n[ \t]*exec quickshell ',a.launcher.read_text(),maxsplit=1)[0]
 env.update(dict(v.decode().split('=',1) for v in sp.check_output(['bash','-c',prefix+'\nexec env -0'],env=env).split(b'\0') if b'=' in v))
 env.update(QT_QPA_PLATFORM='wayland',QT_QPA_PLATFORMTHEME='generic',QT_QUICK_BACKEND='software',QT_ACCESSIBILITY='0',NO_AT_BRIDGE='1',GTK_A11Y='none',QT_NO_XDG_DESKTOP_PORTAL='1',WLR_BACKENDS='headless',WLR_RENDERER='gles2' if a.effects else 'pixman',WLR_RENDERER_FORCE_SOFTWARE='1' if a.effects else '0',WLR_RENDERER_ALLOW_SOFTWARE='1',LIBGL_ALWAYS_SOFTWARE='1',WLR_LIBINPUT_NO_DEVICES='1',GLOBAL_MENU_ENABLED='0',LUMEN_FILE_SERVICE='0',LUMEN_LAUNCH_SCOPES='0')
 docs=home/'Documents';docs.mkdir();env['LUMEN_SEARCH_ROOTS']=json.dumps([str(docs)])
 for name in ('alpha one.txt','beta two.txt','retire three.txt','neighbor four.txt','quoted ; café.txt'):(docs/name).write_text('Preview '+name)
 def png(rgb):
  def chunk(kind,data):return struct.pack('>I',len(data))+kind+data+struct.pack('>I',zlib.crc32(kind+data)&0xffffffff)
  return b'\x89PNG\r\n\x1a\n'+chunk(b'IHDR',struct.pack('>IIBBBBB',4,4,8,2,0,0,0))+chunk(b'IDAT',zlib.compress(b''.join(b'\0'+bytes(rgb)*4 for _ in range(4))))+chunk(b'IEND',b'')
 (docs/'picture.png').write_bytes(png((255,0,0)))
 assets=out/'assets';shutil.copytree(Path(env['LUMEN_NOTIFICATION_HELPER']).parent.parent,assets);source=assets/'shell'
 binary=home/'bin';binary.mkdir();env['PATH']=str(binary)+':'+env['PATH'];env['SEARCH_AUDIT_OUTPUT']=str(out)
 sink=binary/'xdg-open';sink.write_text('#!'+sys.executable+'\nimport json,os,sys,time\nfrom pathlib import Path\nwith (Path(os.environ["SEARCH_AUDIT_OUTPUT"])/"opened.jsonl").open("a") as f:f.write(json.dumps(sys.argv[1:])+"\\n")\nmarker=Path(os.environ["SEARCH_AUDIT_OUTPUT"])/"external.pid"\nif not marker.exists():\n pid=os.fork()\n if pid==0:time.sleep(120);os._exit(0)\n marker.write_text(str(pid))\n');sink.chmod(0o700)
 # Gate only response delivery and provider lifetime. Request dispatch and index
 # code are the exact installed file. No generated rows or activation decisions.
 gate=out/'provider_gate.py';gate.write_text('''import json,os,signal,subprocess as sp,sys,threading,time
from pathlib import Path
out=Path(os.environ['SEARCH_AUDIT_OUTPUT']);child=sp.Popen([sys.executable,sys.argv[1]],stdin=sp.PIPE,stdout=sp.PIPE,text=True)
with (out/'providers.jsonl').open('a') as f:f.write(json.dumps(dict(pid=child.pid,start=Path('/proc/'+str(child.pid)+'/stat').read_text().rsplit(') ',1)[1].split()[19]))+'\\n')
(out/'provider.pid').write_text(str(child.pid));(out/'transport.pid').write_text(str(os.getpid()));requests={};lock=threading.Lock();stopping=False
def stop(signum,frame):
 global stopping
 if (out/'defer-stop').exists():stopping=True
 else:
  child.terminate();child.wait(timeout=5);os._exit(0)
signal.signal(signal.SIGTERM,stop)
def log(kind,data):
 with lock:
  with (out/'transport.jsonl').open('a') as f:f.write(json.dumps(dict(kind=kind,data=data))+'\\n')
def emit(data):
 with lock:print(json.dumps(data),flush=True)
def delayed(data,key):
 while (out/key).exists():time.sleep(.02)
 emit(data);log('released',data)
 if stopping:child.terminate();child.wait(timeout=5);os._exit(0)
def reader():
 for line in child.stdout:
  data=json.loads(line);request=requests.get(data.get('id'),{});log('response',data)
  key='hold-'+request.get('action','')+'-'+str(request.get('query',Path(request.get('path','')).name if request.get('path') else ''))
  if (out/key).exists():threading.Thread(target=delayed,args=(data,key),daemon=True).start()
  else:emit(data)
 os._exit(0)
threading.Thread(target=reader,daemon=True).start()
try:
 for line in sys.stdin:
  data=json.loads(line);requests[data['id']]=data;log('request',data);child.stdin.write(line);child.stdin.flush()
finally:
 child.terminate();child.wait(timeout=5)
''')
 original=(source/'SearchState.qml').read_text();needle='command:[root.python,root.helper]';assert original.count(needle)==1
 modified=original.replace(needle,'command:[root.python,'+json.dumps(str(gate))+',root.helper]');(source/'SearchState.qml').chmod(0o600);(source/'SearchState.qml').write_text(modified)
 (out/'manifest.json').write_text(json.dumps(dict(launcher=str(a.launcher),originalProvider=str(source/'search.py'),providerSha256=hashlib.sha256((source/'search.py').read_bytes()).hexdigest(),originalStateSha256=hashlib.sha256(original.encode()).hexdigest(),modifiedStateSha256=hashlib.sha256(modified.encode()).hexdigest(),transformation='Process command wraps exact installed helper with response-delivery gate',roundtrip=modified.replace('command:[root.python,'+json.dumps(str(gate))+',root.helper]',needle)==original),indent=2))
 palettes=json.loads((assets/'appearance/palette.json').read_text());appearance=home/'state/lumen-desktop/appearance.json';appearance.parent.mkdir(parents=True);appearance.write_text(json.dumps(dict(font=palettes['font'],palette=palettes['light'],preferences=dict(mode='light',reduceMotion=True))))
 qml=out/'shell.qml';qml.write_text('''import QtQuick
import QtQuick.Controls
import QtQuick.Window
import Quickshell
import Quickshell.Io
import "assets/shell" as D
ShellRoot {
 id:root
 D.SearchState {id:search;active:stage.visible && launcher.visible}
 QtObject {id:notifications;property bool dnd:false;property bool available:false}
 QtObject {id:audio;property bool muted:false;property bool available:false}
 QtObject {id:exported;property var state:({menu:null})}
 QtObject {id:stub;property var search:search;property var notifications:notifications;property var audioState:audio;property var exportedMenu:exported;property var entries:[];property date currentDate:new Date();function appLabel(e){return e.name;}function openFiles(path){root.browsed=path;}}
 property string browsed:""
 FloatingWindow {id:stage;visible:true;implicitWidth:1100;implicitHeight:850;title:"Search Lifecycle Fixture";color:"#202024"
  D.AppLauncher {id:launcher;visible:stage.visible;shell:stub;x:70;y:50;onCloseRequested:stage.visible=false}
 }
 function find(item,predicate){if(predicate(item))return item;for(const child of item.children || []){const match=find(child,predicate);if(match)return match;}return null;}
 IpcHandler {target:"fixture"
  function status():string{return JSON.stringify({provider:launcher.provider,results:launcher.results,selected:launcher.selectedIndex,key:launcher.selectedKey,query:launcher.queryText,available:search.available,searching:search.searching,opening:search.opening,error:search.error,preview:search.preview,previewOpen:launcher.previewOpen,visible:stage.visible,browsed:root.browsed});}
  function image():string{const b=root.find(launcher,item=>item.source!==undefined && item.fillMode!==undefined && String(item.source).indexOf("lumen-preview-snapshots-")>=0);if(!b)return "{}";const point=b.mapToItem(stage.contentItem,b.width/2,b.height/2);return JSON.stringify({x:point.x,y:point.y,status:b.status,source:String(b.source)});}
  function reopen():void{stage.visible=true;launcher.focusSearch();}
  function button(text:string):string{const b=root.find(launcher,item=>item.text===text && item.visible);if(!b)return "{}";const point=b.mapToItem(stage.contentItem,b.width/2,b.height/2);return JSON.stringify({x:point.x,y:point.y,enabled:b.enabled});}
 }
}
''')
 def launch(cmd,name,**kw):
  with (out/name).open('w') as log:proc=sp.Popen(cmd,env=env,stdout=log,stderr=sp.STDOUT,start_new_session=True,**kw)
  processes.append(proc);return proc
 def call(*cmd):return sp.check_output(cmd,env=env,text=True,timeout=8).strip()
 def wait(fn,label):
  end=time.monotonic()+12
  while time.monotonic()<end:
   try:value=fn()
   except (OSError,ValueError,sp.SubprocessError):value=None
   if value:return value
   time.sleep(.04)
  raise AssertionError('Search native timeout: '+label)
 def ipc(method,*args):return call('quickshell','-p',str(qml),'ipc','call','fixture',method,*args)
 def state():return json.loads(ipc('status'))
 def keys(*args):call('wtype',*args)
 def query(text):keys('-M','ctrl','a','-m','ctrl');keys(text)
 def rows():return state()['results']
 def settled(text):return wait(lambda:state()['query']==text and not state()['searching'] and rows(),'results '+text)
 def logs():return [json.loads(line) for line in (out/'transport.jsonl').read_text().splitlines()] if (out/'transport.jsonl').exists() else []
 def response(query):return any(row['kind']=='request' and row['data'].get('query')==query for row in logs())
 def opened():return [json.loads(line) for line in (out/'opened.jsonl').read_text().splitlines()] if (out/'opened.jsonl').exists() else []
 def pointer_down(text):
  pos=wait(lambda:json.loads(ipc('button',text)),'button '+text);assert pos['enabled'];call(str(Path(a.sway).parent/'swaymsg'),'-r','seat seat0 cursor set '+str(round(pos['x']))+' '+str(round(pos['y'])));native_pointer.stdin.write('m 0.5 0.5\n');native_pointer.stdin.flush();time.sleep(.12);native_pointer.stdin.write('p\n');native_pointer.stdin.flush();time.sleep(.12)
 def pointer_up():native_pointer.stdin.write('r\n');native_pointer.stdin.flush()
 def pointer(text):pointer_down(text);pointer_up()
 try:
  start_private_bus(env,out,processes)
  conf=out/'sway.conf';conf.write_text('output HEADLESS-1 resolution 1200x900\nxwayland disable\nseat seat0 fallback true\nfocus_follows_mouse no\ndefault_border none\nfor_window [app_id="org.quickshell"] floating enable, move position 0 0\n')
  launch([a.sway,'--unsupported-gpu','-c',str(conf)],'sway.log');wait(lambda:list(home.glob('sway-ipc*.sock')),'compositor')
  env['SWAYSOCK']=str(next(home.glob('sway-ipc*.sock')));env['WAYLAND_DISPLAY']=next(f.name for f in home.glob('wayland-*') if not f.name.endswith('.lock'))
  launch(['wtype','-s','600000'],'keyboard.log');native_pointer=launch([a.pointer_tool,'pointer','move','0','0'],'pointer.log',stdin=sp.PIPE,text=True)
  launch(['quickshell','-p',str(qml)],'shell.log');wait(lambda:state()['available'],'provider ready');call(str(Path(a.sway).parent/'swaymsg'),'-r','[title="^Search Lifecycle Fixture$"] focus');time.sleep(.2)
  pointer('Files');wait(lambda:state()['provider']=='Files','Files category');assert rows()==[];checks.append('empty file category')
  (out/'hold-search-alpha').touch();query('alpha');wait(lambda:response('alpha'),'delayed first query');query('beta');settled('beta');assert all('beta' in row['title'] for row in rows());(out/'hold-search-alpha').unlink();time.sleep(.3);assert all('beta' in row['title'] for row in rows());checks.append('late old query cannot replace current results')
  keys('-M','ctrl','-k','space','-m','ctrl');wait(lambda:state()['preview'].get('type')=='text','preview');assert state()['preview']['text']=='Preview beta two.txt';assert not opened();checks.append('preview selected identity without opening')
  (out/'hold-preview-alpha one.txt').touch();query('alpha');settled('alpha');wait(lambda:any(row['kind']=='request' and row['data'].get('action')=='preview' and row['data'].get('path')==str(docs/'alpha one.txt') for row in logs()),'delayed preview');query('beta');settled('beta');wait(lambda:state()['preview'].get('text')=='Preview beta two.txt','new preview');(out/'hold-preview-alpha one.txt').unlink();time.sleep(.25);assert state()['preview']['text']=='Preview beta two.txt';checks.append('late preview cannot replace newly selected resource')
  keys('-k','Return');wait(lambda:len(opened())==1,'selected open');assert opened()==[[(docs/'beta two.txt').as_uri()]];checks.append('exact selected structured launch')
  ipc('reopen');time.sleep(.15);pointer('Files');query('quoted');settled('quoted');keys('-k','Return');wait(lambda:len(opened())==2,'quoted path open');assert opened()[-1]==[(docs/'quoted ; café.txt').as_uri()];checks.append('non ASCII metacharacter path preserved')
  ipc('reopen');time.sleep(.15);pointer('Files');query('retire');settled('retire');captured=rows()[0];(docs/'retire three.txt').rename(docs/'old-retired');(docs/'retire three.txt').write_text('Replacement must not open');keys('-k','Return');wait(lambda:not state()['opening'],'replacement rejected');assert len(opened())==2 and state()['visible'];checks.append('same path replacement rejected before external dispatch')
  query('neighbor');settled('neighbor');neighborToken=rows()[0]['token'];(out/'hold-search-neighbor ').touch();query('neighbor ');wait(lambda:response('neighbor '),'pending provider query');pid=int((out/'provider.pid').read_text());os.kill(pid,signal.SIGTERM);wait(lambda:not state()['available'],'provider exit');assert not rows() and state()['error'];keys('-k','Return');assert len(opened())==2;checks.append('provider loss clears cached action and prevents launch')
  (out/'hold-search-neighbor ').unlink();wait(lambda:state()['available'] and not state()['searching'] and rows(),'provider reconnect');assert rows()[0]['path']==str(docs/'neighbor four.txt') and rows()[0]['token']!=neighborToken;checks.append('provider restart recovers scoped results')
  query('ext:txt');settled('ext:txt');keys('-k','Down');selected=rows()[state()['selected']];Path(selected['path']).unlink();wait(lambda:all(row['path']!=selected['path'] for row in rows()),'removed selected result');assert state()['selected']==-1;keys('-k','Return');time.sleep(.15);assert len(opened())==2;checks.append('removed selected row never retargets adjacent result')
  before=len(logs());(out/'hold-search-alpha').touch();query('alpha');wait(lambda:any(row['kind']=='request' and row['data'].get('query')=='alpha' for row in logs()[before:]),'category pending query');pointer('Applications');wait(lambda:state()['provider']=='Applications','changed category');(out/'hold-search-alpha').unlink();time.sleep(.2);assert rows()==[];checks.append('late file results do not leak into another category')
  pointer('Applications');wait(lambda:state()['provider']=='Applications','Applications category');assert rows()==[];pointer('Actions');wait(lambda:state()['provider']=='Actions','Actions category');query('Control');wait(lambda:any(row['kind']=='action' for row in rows()),'action category');assert all(row['kind']=='action' for row in rows());checks.append('category transitions preserve provenance')
  pointer('Files');query('neighbor');settled('neighbor');pid=int((out/'provider.pid').read_text());os.kill(pid,signal.SIGSTOP);before=len(logs());query('beta');wait(lambda:any(row['kind']=='request' and row['data'].get('query')=='beta' for row in logs()[before:]),'hung query dispatched');wait(lambda:not state()['available'] and ('unavailable' in state()['error'] or 'timed out' in state()['error']),'bounded hung provider deadline');assert not rows() and not state()['searching'];os.kill(pid,signal.SIGKILL);wait(lambda:state()['available'] and not state()['searching'],'hung provider recovery');assert len(opened())==2;checks.append('SIGSTOP provider retires cached actions within bounded deadline and recovers without open retry')
  pointer('Files');query('quoted');settled('quoted');(out/'hold-open-quoted ; café.txt').touch();(out/'defer-stop').touch();keys('-k','Return');wait(lambda:len(opened())==3 and state()['opening'],'pending open acknowledgement');keys('-k','Escape');wait(lambda:not state()['visible'],'dismiss pending action');ipc('reopen');time.sleep(.15);pointer('Files');query('neighbor');(out/'hold-open-quoted ; café.txt').unlink();wait(lambda:any(row['kind']=='released' and row['data'].get('action')=='open' for row in logs()),'retired action reply');(out/'defer-stop').unlink();settled('neighbor');assert state()['visible'] and len(opened())==3 and not state()['opening'];checks.append('retired open acknowledgement cannot dismiss new invocation or retry action');external=int((out/'external.pid').read_text());assert Path('/proc/'+str(external)+'/stat').read_text().rsplit(') ',1)[1][0]!='Z';checks.append('external launch observer survives exact provider retirement')
  pointer('Files');(out/'hold-preview-picture.png').touch();query('picture');settled('picture');
  if not state()['previewOpen']:keys('-M','ctrl','-k','space','-m','ctrl')
  wait(lambda:any(row['kind']=='response' and row['data'].get('action')=='preview' and row['data'].get('name')=='picture.png' for row in logs()),'retained image response');(out/'hold-search-picture').touch();replacement=docs/'replacement.png';replacement.write_bytes(png((0,0,255)));os.replace(replacement,docs/'picture.png');(out/'hold-preview-picture.png').unlink();image=wait(lambda:(value if (value:=json.loads(ipc('image'))).get('status')==1 else None),'actual image loaded');snapshot=Path(image['source'].removeprefix('file://'));assert snapshot.read_bytes()==png((255,0,0)) and (docs/'picture.png').read_bytes()==png((0,0,255));
  def rendered_original():
   call('grim','-o','HEADLESS-1',str(out/'retained-image.png'))
   pixel=call('magick',str(out/'retained-image.png'),'-format','%[pixel:p{'+str(round(image['x']))+','+str(round(image['y']))+'}]','info:')
   return pixel if pixel in ('srgb(255,0,0)','srgba(255,0,0,1)') else None
  pixel=wait(rendered_original,'retained pixels presented');(out/'retained-image-proof.json').write_text(json.dumps(dict(image=image,pixel=pixel,retainedSha256=hashlib.sha256(snapshot.read_bytes()).hexdigest(),replacementSha256=hashlib.sha256((docs/'picture.png').read_bytes()).hexdigest())));checks.append('delayed image response loads retained original pixels after source replacement')
  (out/'hold-search-picture').unlink();query('ext:txt');settled('ext:txt');captured=rows()[state()['selected']];pointer_down('Open');keys('-k','Down');assert rows()[state()['selected']]['path']!=captured['path'];pointer_up();wait(lambda:len(opened())==4,'captured preview button');assert opened()[-1]==[Path(captured['path']).as_uri()];checks.append('held preview Open button preserves pressed resource despite keyboard selection change')
  ipc('reopen');wait(lambda:state()['available'],'fresh quota provider');pointer('Files')
  for index in range(1,4):
   with (docs/('quota'+str(index)+'.bin')).open('wb') as stream:stream.truncate(64*1024*1024)
  for index in (1,2):
   query('quota'+str(index));settled('quota'+str(index))
   if not state()['previewOpen']:keys('-M','ctrl','-k','space','-m','ctrl')
   wait(lambda:state()['preview'].get('text')=='No preview available','accepted bounded snapshot '+str(index))
  query('quota3');settled('quota3');wait(lambda:'unavailable' in state()['preview'].get('text',''),'quota exhausted');assert state()['visible'];checks.append('real 128 MiB snapshot quota refuses new copy without evicting published resources')
  previous=int((out/'provider.pid').read_text());keys('-k','Escape');wait(lambda:not state()['visible'],'quota dismissal');ipc('reopen');wait(lambda:state()['available'] and int((out/'provider.pid').read_text())!=previous,'new quota provider');pointer('Files');query('quota3');settled('quota3');keys('-M','ctrl','-k','space','-m','ctrl');wait(lambda:state()['preview'].get('text')=='No preview available','quota reset after cached reopen');assert len(opened())==4;checks.append('actual dismissal reopens cached index with fresh snapshot quota and no action replay')
  result=dict(passed=True,checks=checks,launcher=str(a.launcher),sway=a.sway,effects=a.effects,hostClipboardUntouched=True,externalOpenBoundary='Validated selected identity at dispatch; external handler may reopen the path later.')
  (out/'result.json').write_text(json.dumps(result,indent=2));print(json.dumps(result))
 finally:
  # A deliberately stopped private provider must be reaped even on failed input
  # assertions. Retire only captured PID/start identities, never other jobs.
  identities=out/'providers.jsonl'
  if identities.exists():
   for line in identities.read_text().splitlines():
    try:
     facts=json.loads(line);current=Path('/proc/'+str(facts['pid'])+'/stat').read_text().rsplit(') ',1)[1].split()[19]
     if current==facts['start']:os.kill(facts['pid'],signal.SIGKILL)
    except (OSError,ValueError,IndexError):pass
  # Provider belongs to this private process group; no unrelated jobs are killed.
  for proc in reversed(processes):
   if proc.poll() is None:
    try:os.killpg(proc.pid,signal.SIGTERM)
    except ProcessLookupError:pass
  for proc in reversed(processes):
   try:proc.wait(timeout=5)
   except sp.TimeoutExpired:
    os.killpg(proc.pid,signal.SIGKILL);proc.wait(timeout=3)
   for stream in (proc.stdin,proc.stdout,proc.stderr):
    if stream:stream.close()

"""Private actual Wayland URI offers through the production Dock drop helper."""
import argparse,hashlib,json,os,re,shutil,signal,subprocess as sp,sys,tempfile,time
from pathlib import Path
from application_fixture.isolation import run as isolated_run
from application_fixture.provenance import FrozenSource,generated
from private_native_bus import start_private_bus
from fixture_resources import close_reaped_pipes
p=argparse.ArgumentParser();p.add_argument('--busy-case',action='store_true');p.add_argument('--launcher',type=Path,required=True);p.add_argument('--sway',type=Path,required=True);p.add_argument('--output',type=Path);p.add_argument('--effects',action='store_true');p.add_argument('--inside',action='store_true');p.add_argument('--python-runtime',default=shutil.which('desktop-test-python'));p.add_argument('--egl-vendor',type=Path,default=os.environ.get('__EGL_VENDOR_LIBRARY_FILENAMES'));p.add_argument('--pointer',type=Path,required=True);p.add_argument('--font-config',type=Path,required=True);p.add_argument('--source-revision');p.add_argument('--parent-net');a=p.parse_args()
if not a.inside:
 if not a.output or not a.python_runtime:p.error('Fresh artifacts and pinned test runtime required')
 a.output=a.output.resolve();a.output.mkdir(mode=0o700,parents=True,exist_ok=False)
 a.launcher=a.launcher.resolve();a.sway=a.sway.resolve();a.pointer=a.pointer.resolve();a.font_config=a.font_config.resolve();runtime=Path(a.python_runtime).resolve()
 if not all(str(path).startswith('/nix/store/') for path in (a.launcher,a.sway,a.pointer,a.font_config,runtime)):raise ValueError('Pinned native tools required')
 frozen=FrozenSource(Path(__file__).resolve().parents[2],__file__,[a.launcher,a.sway,a.pointer,a.font_config,runtime]+([a.egl_vendor] if a.effects else []));revision=frozen.revision
 args=[str(runtime),'/source/tests/desktop/dock_offer_native.py','--inside','--launcher',str(a.launcher),'--sway',str(a.sway),'--pointer',str(a.pointer),'--font-config',str(a.font_config),'--source-revision',revision,'--parent-net',os.readlink('/proc/self/ns/net')]
 if a.busy_case:args+=['--busy-case']
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
with tempfile.TemporaryDirectory(prefix='lumen-dock-offer-') as temporary:
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
  if 'peerqml' in globals():(a.output/'timeout-peer.json').write_text(json.dumps(peer_state(),indent=2))
  (a.output/'timeout-tree.json').write_text(json.dumps(tree(),indent=2))
  (a.output/'timeout-seats.json').write_text(json.dumps(sway('-t','get_seats'),indent=2))
  raise AssertionError('Dock offer observation deadline')
 def nodes(node):
  yield node
  for child in node.get('nodes',[])+node.get('floating_nodes',[]):yield from nodes(child)



 def ui():return json.loads(call('quickshell','-p',qml,'ipc','call','dockOffer','status'))
 def peer_state():return json.loads(call('quickshell','-p',peerqml,'ipc','call','offerPeer','status'))
 def peer_ipc(method,*args):return call('quickshell','-p',peerqml,'ipc','call','offerPeer',method,*args)
 def records():return [json.loads(line) for line in opened.read_text().splitlines()] if opened.exists() else []
 position=[0,0]
 def move(x,y):pointer.stdin.write('m %.5f %.5f\n'%(x-position[0],y-position[1]));pointer.stdin.flush();position[:]=[x,y]
 def button(value):pointer.stdin.write(value+'\n');pointer.stdin.flush()
 def native(cid):return next((node for node in nodes(tree()) if node.get('id')==cid),None)
 def target(key):
  state=ui();index=state['keys'].index(key);return ((1280-state['width'])/2+12+(index+.5)*state['slot'],900-7-state['shelf']/2)
 def start_offer(uris,key,accepted=True):
  assert len(uris)==2;ack=peer_ipc('configureUris',*uris);assert ack=='true',(ack,peer_state(),uris);wait(lambda:peer_state()['uris']==uris and peer_state()['mimeBytes']==sum(len(uri)+2 for uri in uris));node=native(cid);checked('[con_id=%d] focus'%cid);wait(lambda:native(cid).get('focused'))
  state=peer_state();node=native(cid);assert node['window_rect']['width']==state['width'] and node['window_rect']['height']==state['height'],(node,state)
  x=node['rect']['x']+node['window_rect']['x']+state['x'];y=node['rect']['y']+node['window_rect']['y']+state['y'];move(x,y);wait(lambda:peer_state()['hovered']);button('p');wait(lambda:peer_state()['pressed']);move(x+16,y);move(x+32,y);wait(lambda:peer_state()['active']);move(*target(key));
  if accepted:wait(lambda:any(row['key']==key and row['contains'] for row in ui()['drops']))
  else:wait(lambda:any(seat.get('lumen_drag',{}).get('active') for seat in sway('-t','get_seats')))
 checks=[]
 try:
  start_private_bus(env,a.output,processes)
  config=private/'sway.conf';config.write_text('output HEADLESS-1 resolution 1280x900 position 0 0\noutput HEADLESS-2 disable\nseat seat0 fallback true\nxwayland disable\nfocus_follows_mouse no\nfor_window [all] floating enable, border normal 1\n')
  launch([str(a.sway),'--unsupported-gpu','-c',str(config)],'sway.log');env['SWAYSOCK']=str(wait(lambda:next(Path(env['XDG_RUNTIME_DIR']).glob('sway-ipc*.sock'),None)));env['WAYLAND_DISPLAY']=wait(lambda:next((x.name for x in Path(env['XDG_RUNTIME_DIR']).glob('wayland-*') if not x.name.endswith('.lock')),None))
  pointer=launch([str(a.pointer),'pointer','move','0','0'],'pointer.log',stdin=sp.PIPE,text=True);pointer.stdin.write('m 0 0\n');pointer.stdin.flush();launch(['wtype','-s','120000'],'keyboard.log');wait(lambda:any(seat.get('capabilities',0)&1 for seat in sway('-t','get_seats')));checked('seat seat0 cursor set 0 0');pointer.stdin.write('m 0 0\n');pointer.stdin.flush()
  env.update(LUMEN_LAUNCH_SCOPES='0',XDG_CURRENT_DESKTOP='sway',DBUS_SYSTEM_BUS_ADDRESS='unix:path='+str(private/'absent-system'),PULSE_SERVER='unix:'+str(private/'absent-audio'),XDG_DATA_DIRS=env['XDG_DATA_HOME'])
  docs=Path(env['HOME'])/'offers';docs.mkdir();alpha=docs/'Alpha # % ü.txt';alpha.write_text('first captured offer');beta=docs/'Beta.txt';beta.write_text('second captured offer');image=docs/'pixel.png'
  from PIL import Image
  Image.new('RGB',(8,8),'red').save(image);originals={str(path):hashlib.sha256(path.read_bytes()).hexdigest() for path in (alpha,beta,image)}
  opened=a.output/'opened.jsonl';observer=private/'observer.py';observer.write_text('''import hashlib,json,sys
from pathlib import Path
from urllib.parse import unquote,urlsplit
rows=[]
for uri in sys.argv[2:]:
 value=urlsplit(uri)
 if value.scheme=='file':
  assert not value.netloc;path=Path(unquote(value.path))
 else:
  assert not value.scheme;path=Path(uri)
 assert path.is_absolute();rows.append(dict(argument=uri,uri=path.as_uri(),path=str(path),sha256=hashlib.sha256(path.read_bytes()).hexdigest()))
with Path(sys.argv[1]).open('a') as out:out.write(json.dumps(rows)+chr(10))
''')
  gate=private/'release-helper';gate_record=a.output/'busy-child.json'
  if a.busy_case:
   real_helper=Path(shutil.which('lumen-dock-drop',path=env['PATH'])).resolve();assert str(real_helper).startswith('/nix/store/')
   bindir=private/'bin';bindir.mkdir();wrapper=bindir/'lumen-dock-drop'
   wrapper.write_text('#!'+sys.executable+'\n'+"import json,os,sys,time\nfrom pathlib import Path\n"+f"gate=Path({str(gate)!r});record=Path({str(gate_record)!r})\n"+"temporary=record.with_suffix('.tmp');temporary.write_text(json.dumps(dict(pid=os.getpid(),argv=sys.argv[1:])));temporary.replace(record)\ndeadline=time.clock_gettime(time.CLOCK_BOOTTIME)+12\nwhile not gate.exists():\n if time.clock_gettime(time.CLOCK_BOOTTIME)>=deadline:\n  print(json.dumps(dict(error='Private helper gate expired')));sys.exit(1)\n time.sleep(.01)\n"+f"os.execv({str(real_helper)!r},[{str(real_helper)!r},*sys.argv[1:]])\n")
   wrapper.chmod(0o700);env['PATH']=str(bindir)+os.pathsep+env['PATH'];(a.output/'busy-helper.json').write_text(json.dumps(dict(helper=str(real_helper),sha256=hashlib.sha256(real_helper.read_bytes()).hexdigest(),scope='Fixed private gate forwards unchanged argv to captured installed helper only after explicit release.'),indent=2))
  apps=Path(env['XDG_DATA_HOME'])/'applications';apps.mkdir();(apps/'fixture-accept.desktop').write_text('[Desktop Entry]\nType=Application\nName=Offer Observer\nExec='+sys.executable+' '+str(observer)+' '+str(opened)+' %U\nMimeType=text/plain;\nTerminal=false\n');(apps/'fixture-refuse.desktop').write_text('[Desktop Entry]\nType=Application\nName=Unsupported Observer\nExec='+sys.executable+' '+str(observer)+' '+str(opened)+' %U\nMimeType=image/png;\nTerminal=false\n')
  assets=private/'assets';shutil.copytree(repo/'res/lumen-desktop',assets);targetfile=assets/'shell/Dock.qml';original=targetfile.read_text();needle='    id: dock';assert original.count(needle)==1
  addition='''
    property alias fixtureDropWorker:dropWorker
    property var fixtureEvents:[]
    function fixtureRecord(kind,key,event){const offered=event.urls;fixtureEvents=fixtureEvents.concat([{kind:kind,key:key,hasUrls:event.hasUrls,length:offered.length,urls:Array.from(offered).map(url=>url.toString()),mapped:offered.map(url=>url.toString()),supportedActions:event.supportedActions}]);}
    function fixtureDrops(){const result=[];function visit(item){if(item.modelData && item.index!==undefined){const area=item.children.find(child=>child.containsDrag!==undefined);if(area)result.push({key:item.modelData.key,contains:area.containsDrag});}for(const child of item.children || [])visit(child);}visit(surface);return result;}
'''
  entered='onEntered:drag=>{';dropped='onDropped:drop=>{';assert original.count(entered)==1 and original.count(dropped)==1;enteredHook='dock.fixtureRecord("enter",tile.modelData.key,drag);';droppedHook='dock.fixtureRecord("drop",tile.modelData.key,drop);';modified=original.replace(needle,needle+addition,1).replace(entered,entered+enteredHook,1).replace(dropped,dropped+droppedHook,1);targetfile.chmod(0o600);targetfile.write_text(modified);(a.output/'instrumentation.json').write_text(json.dumps(dict(originalSha256=hashlib.sha256(original.encode()).hexdigest(),modifiedSha256=hashlib.sha256(modified.encode()).hexdigest(),roundtrip=modified.replace(addition,'',1).replace(enteredHook,'',1).replace(droppedHook,'',1)==original,scope='Read-only actual DropArea.containsDrag collector, enter/drop metadata records and worker alias only; production MIME handling, receiver and subprocess command unchanged.'),indent=2))
  palette=json.loads((assets/'appearance/palette.json').read_text());appearance=private/'appearance.json';appearance.write_text(json.dumps(dict(font=palette['font'],palette=palette['dark'],preferences=dict(reduceMotion=True,reduceTransparency=True))));env['LUMEN_APPEARANCE_STATE']=str(appearance)
  qml=private/'dock.qml';qml.write_text('''import QtQuick
import Quickshell
import Quickshell.Io
import "assets/shell" as D
ShellRoot {QtObject {id:stub
 property string popupOwner:""
 property date currentDate:new Date()
 property bool dockPreferencesBusy:false
 property string aliasError:""
 property string applicationCommandError:""
 property var referenceDock:[{key:"accept",entry:{id:"fixture-accept",name:"Offer Observer",icon:"application-x-executable"},canvas:68},{key:"refuse",entry:{id:"fixture-refuse",name:"Unsupported Observer",icon:"application-x-executable"},canvas:68}]
 signal requestDockFocus(string name)
 signal dismissPopups(string owner)
 function claimPopup(owner){popupOwner=owner;dismissPopups(owner);}
 function dockWindows(item){return [];}
 function dockLabel(item){return item.key;}
 function runningFor(entry){return false;}
 function activateDock(){throw Error("Unexpected activation");}
 function canConfigureDockEntry(){return false;}
 function isDockPinned(){return false;}
}
 D.Dock {id:dock;shell:stub;screen:Quickshell.screens[0]}
 IpcHandler {target:"dockOffer";function status():string{return JSON.stringify({keys:stub.referenceDock.map(row=>row.key),width:dock.width,slot:dock.slot,shelf:dock.shelfThickness,events:dock.fixtureEvents,command:dock.fixtureDropWorker.command,error:dock.dropError,busy:dock.fixtureDropWorker.running,drops:dock.fixtureDrops()});}}
}''')
  peerqml=private/'peer.qml';peerqml.write_text('''import QtQuick
import Quickshell
import Quickshell.Io
ShellRoot {id:root
 property var uris:[]
 property int finished:0
 property int action:0
 function uriBytes(){const delimiter=String.fromCharCode(13,10),text=uris.join(delimiter)+delimiter,bytes=new Uint8Array(text.length);for(let i=0;i<text.length;i++){const value=text.charCodeAt(i);if(value>127)throw Error("URI encoding required");bytes[i]=value;}return bytes.buffer;}
 FloatingWindow {id:window;visible:true;title:"Private URI offer peer";implicitWidth:500;implicitHeight:320;color:"#dce4ee"
  Rectangle {id:offer;x:50;y:50;width:200;height:100;color:"#4080cc"
   Text {anchors.centerIn:parent;text:"Private URI offer";color:"white"}
   Drag.active:mouse.drag.active;Drag.dragType:Drag.Automatic;Drag.supportedActions:Qt.CopyAction;Drag.proposedAction:Qt.CopyAction
   Drag.mimeData:({"text/uri-list":root.uriBytes()})
   Drag.onDragFinished:dropAction=>{root.finished++;root.action=dropAction;offer.x=50;offer.y=50;}
   MouseArea {id:mouse;anchors.fill:parent;hoverEnabled:true;drag.target:offer;drag.threshold:8}
  }
 }
 IpcHandler {target:"offerPeer"
  function configureUris(first:string,second:string):bool{root.uris=[first,second];return root.uris.length===2 && first.length>0 && second.length>0;}
  function status():string{const at=offer.mapToItem(window.contentItem,offer.width/2,offer.height/2);return JSON.stringify({uris:root.uris,mimeBytes:offer.Drag.mimeData["text/uri-list"].byteLength,finished:root.finished,action:root.action,active:mouse.drag.active,pressed:mouse.pressed,hovered:mouse.containsMouse,x:at.x,y:at.y,width:window.width,height:window.height});}
 }
}''')
  launch(['quickshell','-p',str(qml)],'shell.log');wait(lambda:len(ui()['drops'])==2);peer=launch(['quickshell','-p',str(peerqml)],'peer.log');node=wait(lambda:next((n for n in nodes(tree()) if n.get('name')=='Private URI offer peer' and n.get('pid')==peer.pid and n.get('window_rect',{}).get('width',0)>0),None));cid=node['id'];wait(lambda:peer_state()['width']>0)
  uris=[alpha.as_uri(),beta.as_uri()]
  if a.busy_case:
   start_offer(uris,'accept');button('r');wait(lambda:peer_state()['finished']==1 and ui()['busy'] and gate_record.exists());first=ui()['command'];assert peer_state()['action']==1 and not records()
   later=docs/'Later.txt';later.write_text('later unaccepted offer');second=[beta.as_uri(),later.as_uri()]
   # The baseline has no busy message; wait only actual native source completion.
   start_offer(second,'accept',accepted=False);button('r');wait(lambda:peer_state()['finished']==2)
   observed=dict(peer=peer_state(),ui=ui(),firstCommand=first,gate=json.loads(gate_record.read_text()),launches=records());(a.output/'busy-observed.json').write_text(json.dumps(observed,indent=2))
   assert peer_state()['action']==0,observed
   assert ui()['error'] and ui()['command']==first and not records();call('grim',a.output/'busy-refused.png');checks.append('Busy native drop reports IgnoreAction with visible refusal and no queued or replacement helper')
   gate.touch();wait(lambda:not ui()['busy'] and len(records())==1);assert [row['uri'] for row in records()[0]]==uris;checks.append('Releasing the first owned helper dispatches only its original captured URI batch exactly once')
   assert all(hashlib.sha256(Path(path).read_bytes()).hexdigest()==digest for path,digest in originals.items());checks.append('Busy refusal preserves all original source bytes')
  else:
   uris=[alpha.as_uri(),beta.as_uri()];start_offer(uris,'accept');button('r');wait(lambda:peer_state()['finished']==1 and not ui()['busy'] and (len(records())==1 or ui()['error']));assert not ui()['error'] and len(records())==1,ui();rows=records()[0];assert [row['uri'] for row in rows]==uris and {row['path']:row['sha256'] for row in rows}=={str(path):originals[str(path)] for path in (alpha,beta)};assert peer_state()['action']==1;checks.append('Actual separate Wayland multi-file URI offer reaches production Dock/helper and one private registered application with exact original bytes')
   start_offer([alpha.as_uri(),image.as_uri()],'accept');button('r');wait(lambda:peer_state()['finished']==2 and ui()['error'] and not ui()['busy']);assert len(records())==1 and 'every dropped file type' in ui()['error'];checks.append('Mixed compatible/unsupported MIME offer is rejected as a whole without partial application launch')
   start_offer(uris,'refuse');button('r');wait(lambda:peer_state()['finished']==3 and ui()['error'] and not ui()['busy']);assert len(records())==1;checks.append('Actual offer over a different incompatible app cannot fall back to another receiver')
   unavailable=docs/'Unavailable.txt';assert not unavailable.exists();start_offer([alpha.as_uri(),unavailable.as_uri()],'accept');button('r');wait(lambda:peer_state()['finished']==4 and ui()['error'] and not ui()['busy']);assert len(records())==1 and 'unavailable or unreadable' in ui()['error'];checks.append('An actual offer containing an inaccessible local path is refused before any partial app launch')
   beforeCommand=ui()['command'];beforeDrops=sum(event['kind']=='drop' for event in ui()['events']);start_offer(uris,'accept');call('wtype','-k','Escape');wait(lambda:peer_state()['finished']==5 and not any(row['contains'] for row in ui()['drops']));button('r');assert len(records())==1 and not ui()['busy'] and ui()['command']==beforeCommand and sum(event['kind']=='drop' for event in ui()['events'])==beforeDrops;checks.append('Physical Escape cancels the actual native offer without another helper dispatch')
   assert all(hashlib.sha256(Path(path).read_bytes()).hexdigest()==digest for path,digest in originals.items());checks.append('Accepted, refused and canceled offers leave all source files unchanged')
  call('grim',a.output/'accepted.png');logs=''.join(path.read_text() for path in a.output.glob('*.log'));assert not any(word in logs for word in ('ReferenceError','TypeError','Binding loop','Failed to load')),logs
  (a.output/'result.json').write_text(json.dumps(dict(passed=True,checks=checks,source=a.source_revision,compositor=str(a.sway),launches=records(),final=ui(),peer=peer_state(),scope='Actual disposable local URI offers and registered subprocess observer; no authenticated app identity or immutable external handler reopen guarantee.',liveSocketUnchanged=os.environ.get('SWAYSOCK')==original_socket),indent=2));print(json.dumps(dict(passed=True,checks=len(checks))))
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

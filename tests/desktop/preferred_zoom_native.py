"""Private native preferred-client Zoom, placement restore and titlebar gestures."""
import argparse,hashlib,json,os,re,shutil,signal,subprocess as sp,tempfile,time
from pathlib import Path
from fixture_resources import close_reaped_pipes
from private_native_bus import start_private_bus
p=argparse.ArgumentParser();p.add_argument('--launcher',type=Path,required=True);p.add_argument('--sway',required=True);p.add_argument('--pointer',required=True);p.add_argument('--effects',action='store_true');p.add_argument('--rapid',action='store_true');p.add_argument('--output',type=Path,required=True);a=p.parse_args()
out=a.output.resolve();out.mkdir(parents=True,exist_ok=False);processes=[];records=[]
private=tempfile.TemporaryDirectory(prefix='lumen-preferred-zoom-');env=dict(os.environ,HOME=private.name,XDG_RUNTIME_DIR=private.name)
for key in ('CONFIG','STATE','CACHE','DATA'):env['XDG_'+key+'_HOME']=str(Path(private.name)/key.lower())
for key in ('DISPLAY','WAYLAND_DISPLAY','SWAYSOCK','DBUS_SESSION_BUS_ADDRESS','AT_SPI_BUS_ADDRESS'):env.pop(key,None)
prefix=re.split(r'\n[ \t]*exec quickshell ',a.launcher.read_text(),maxsplit=1)[0]
env.update(dict(value.decode().split('=',1) for value in sp.check_output(['bash','-c',prefix+'\nexec env -0'],env=env).split(b'\0') if b'=' in value))
env.update(WLR_BACKENDS='headless',WLR_HEADLESS_OUTPUTS='2',WLR_RENDERER='gles2' if a.effects else 'pixman',WLR_RENDERER_FORCE_SOFTWARE='1' if a.effects else '0',WLR_RENDERER_ALLOW_SOFTWARE='1',LIBGL_ALWAYS_SOFTWARE='1',WLR_LIBINPUT_NO_DEVICES='1',QT_QPA_PLATFORM='wayland',QT_QPA_PLATFORMTHEME='generic',QT_QUICK_BACKEND='software',QT_ACCESSIBILITY='0',NO_AT_BRIDGE='1',GTK_A11Y='none')
def launch(argv,name,**kwargs):
 with (out/name).open('w') as log:proc=sp.Popen(argv,env=env,stdout=log,stderr=sp.STDOUT,start_new_session=True,**kwargs)
 processes.append(proc);return proc
def call(*argv):return sp.check_output(argv,env=env,text=True,timeout=5).strip()
def raw(command):
 result=sp.run(['swaymsg','-r',command],env=env,text=True,stdout=sp.PIPE,stderr=sp.PIPE,timeout=5)
 assert result.returncode in (0,2),(result.returncode,result.stderr)
 return json.loads(result.stdout)
def ipc(command):
 result=raw(command);assert all(row.get('success') for row in result),result
 return result
def nodes(node):
 yield node
 for child in node.get('nodes',[])+node.get('floating_nodes',[]):yield from nodes(child)
def tree():return json.loads(call('swaymsg','-r','-t','get_tree'))
def subject():return next((row for row in nodes(tree()) if row.get('app_id')=='org.quickshell' and row.get('name')=='Preferred Zoom Fixture'),None)
def wait(fn,label):
 deadline=time.monotonic()+10
 while time.monotonic()<deadline:
  value=fn()
  if value:return value
  time.sleep(.03)
 (out/'failed-records.json').write_text(json.dumps(records,indent=2));(out/'failed-tree.json').write_text(json.dumps(tree(),indent=2));raise AssertionError(label)
def saved(node):return {key:node[key] for key in ('rect','window_rect','deco_rect')}
def client(node):return (node['window_rect']['width'],node['window_rect']['height'])
config=out/'sway.conf';config.write_text('font sans 10\noutput HEADLESS-1 resolution 1280x900 position 0 0\noutput HEADLESS-2 resolution 800x600 position 1280 0\noutput * bg #29445b solid_color\nfor_window [app_id="org.quickshell"] floating enable\ndefault_floating_border normal 2\n')
qml=out/'subject.qml';qml.write_text('''import QtQuick
import QtQuick.Window
import Quickshell
import Quickshell.Io
ShellRoot {id:fixture
 property string response:""
 Process {id:registration;stdout:StdioCollector {onStreamFinished:fixture.response=text}}
 IpcHandler {target:"zoomFixture"
  function register(container:int,title:string):void {fixture.response="";registration.command=["lumen-window-geometry","--app","org.quickshell","--title",title,"--container",String(container),"--outer-width","1200","--outer-height","820","--minimum-width","1","--minimum-height","1","--preferred-width","640","--preferred-height","440"];registration.running=true;}
  function state():string {return JSON.stringify({response:fixture.response,busy:registration.running,width:window.width,height:window.height,clientWidth:resizable.width,clientHeight:resizable.height});}
  function minSize(width:int,height:int):void {window.minimumSize=Qt.size(width,height);}
  function title(value:string):void {window.title=value;}
  function showClient(value:bool):void {resizable.visible=value;}
  function resizeClient(width:int,height:int):void {resizable.width=width;resizable.height=height;}
 }
 Window {id:resizable;visible:false;title:"Client Resize Fixture";width:420;height:300;Rectangle {anchors.fill:parent;color:"#a2e3f3"}}
 FloatingWindow {id:window; onWidthChanged:console.log("client-size",Date.now(),width,height);onHeightChanged:console.log("client-size",Date.now(),width,height); visible:true; title:"Preferred Zoom Fixture"; implicitWidth:420; implicitHeight:300
 Rectangle {anchors.fill:parent;color:"#f3e3a2";Text {anchors.centerIn:parent;text:"Preferred client content";color:"#202020";font.pixelSize:22}}
}}''')
try:
 start_private_bus(env,out,processes);launch([a.sway,'-d','--unsupported-gpu','-c',str(config)],'compositor.log');wait(lambda:list(Path(private.name).glob('sway-ipc.*.sock')),'compositor socket');env['SWAYSOCK']=str(next(Path(private.name).glob('sway-ipc.*.sock')));env['WAYLAND_DISPLAY']=next(path.name for path in Path(private.name).glob('wayland-*') if not path.name.endswith('.lock'))
 assert json.loads(call('swaymsg','-r','-t','get_version'))['native_preferred_zoom'];ipc('lumen_window margins disable');ipc('focus output HEADLESS-1');owner=launch(['quickshell','-p',str(qml)],'subject.log');node=wait(subject,'owned subject');cid=node['id'];assert node['pid']==owner.pid
 identity=f'[con_id={cid} pid={owner.pid} app_id="^org[.]quickshell$" title="^Preferred Zoom Fixture$"] '
 def invoke(method,*args):return call('quickshell','-p',str(qml),'ipc','call','zoomFixture',method,*map(str,args))
 def action(name):
  result=ipc(identity+'lumen_window '+name)
  if name not in ('minimize',):
   wait(lambda:(lambda state:(state['width'],state['height'])==client(subject()))(json.loads(invoke('state'))),'client/native size agreement after '+name)
  return result
 def reset():
  if subject()['lumen_state'].get('filled'):action('restore')
  ipc(identity+'resize set 460 px 340 px, move absolute position 180 px 180 px');wait(lambda:subject()['rect']['x']==180 and subject()['rect']['width']==460,'reset placement');return saved(subject())
 before=reset();failure=raw(identity+'lumen_window zoom');assert not all(row.get('success') for row in failure);assert saved(subject())==before
 records.append(dict(case='unregistered rejects without resize',before=before,result=failure))
 foreign_process=launch(['foot','-o','resize-by-cells=no','--app-id=foreign-zoom-fixture','sleep','60'],'foreign.log')
 def foreign():return next((row for row in nodes(tree()) if row.get('app_id')=='foreign-zoom-fixture'),None)
 foreign_node=wait(foreign,'actual unregistered foreign application');foreign_before=saved(foreign_node);foreign_identity=f"[con_id={foreign_node['id']} pid={foreign_node['pid']}] "
 failure=raw(foreign_identity+'lumen_window zoom');assert not all(row.get('success') for row in failure) and saved(foreign())==foreign_before and not foreign()['lumen_state'].get('preferred_zoom')
 records.append(dict(case='actual foreign app has no inferred preferred size',window=foreign(),response=failure));ipc(foreign_identity+'lumen_window close');wait(lambda:foreign() is None,'foreign disposable app closes')
 ipc(identity+'focus')

 def register(container,title):
  invoke('register',container,title)
  def finished():
   state=json.loads(invoke('state'));return json.loads(state['response']) if state['response'] and not state['busy'] else None
  return wait(finished,'captured owner helper response')
 registration=register(cid,'Preferred Zoom Fixture');assert registration.get('preferredSize')==dict(width=640,height=440) and registration['container']==cid and registration['pid']==owner.pid,registration
 records.append(dict(case='actual same-owner helper preferred registration',response=registration))
 assert wait(lambda:subject()['lumen_state'].get('preferred_zoom'),'preferred capability')
 action('zoom');wait(lambda:client(subject())==(640,440) and subject()['lumen_state'].get('zoomed'),'preferred client dimensions');zoom=saved(subject());assert zoom!=before
 call('grim','-o','HEADLESS-1',str(out/'preferred-zoom.png'));action('zoom');wait(lambda:saved(subject())==before,'second Zoom restores exact original');records.append(dict(case='registered client size and toggle restore',original=before,zoom=zoom))
 action('fill');wait(lambda:subject()['lumen_state'].get('filled'),'Fill');filled=saved(subject());assert filled!=zoom and not subject()['lumen_state'].get('zoomed');action('restore');wait(lambda:saved(subject())==before,'Fill restore')
 for placement in ('fill','left'):
  action('zoom');wait(lambda:subject()['lumen_state'].get('zoomed'),'Zoom before placement');action(placement);wait(lambda:not subject()['lumen_state'].get('zoomed'),'placement clears Zoom');action('restore');wait(lambda:saved(subject())==before,'placement retains original restore');records.append(dict(case='Zoom then '+placement+' retains original restore'))
 if a.rapid:
  trace=[]
  for command in ('zoom','fill','restore'):
   ipc(identity+'lumen_window '+command);trace.append(dict(command=command,timeNs=time.monotonic_ns(),native=subject()));(out/'rapid-transcript.json').write_text(json.dumps(dict(original=before,steps=trace),indent=2))
  (out/'rapid-client-after.json').write_text(invoke('state'))
  wait(lambda:saved(subject())==before and (lambda state:(state['width'],state['height'])==client(subject()))(json.loads(invoke('state'))),'rapid placement preserves original restore after client commits')
  records.append(dict(case='rapid Zoom Fill Restore preserves original after commits',steps=trace))
  action('fill');assert subject()['pid']==owner.pid and owner.poll() is None
  stopped=False
  try:
   os.kill(owner.pid,signal.SIGSTOP);stopped=True
   ipc(identity+'lumen_window restore');restore_target=subject()
   ipc(identity+'resize set width 540 px height 400 px');latest_target=saved(subject())
  finally:
   if stopped:os.kill(owner.pid,signal.SIGCONT)
  wait(lambda:saved(subject())==latest_target and (lambda state:(state['width'],state['height'])==client(subject()))(json.loads(invoke('state'))),'latest native target survives older restore acknowledgement')
  records.append(dict(case='new native resize survives stale restore acknowledgement',restore=restore_target,latest=latest_target,received=json.loads(invoke('state'))))
  invoke('showClient','true')
  def resize_subject():return next((row for row in nodes(tree()) if row.get('name')=='Client Resize Fixture' and row.get('pid')==owner.pid),None)
  resizable=wait(resize_subject,'ordinary Qt client');resize_id=resizable['id'];resize_identity=f'[con_id={resize_id} pid={owner.pid}] ';original_client=client(resizable)
  ipc(resize_identity+'lumen_window fill');wait(lambda:(lambda state:(state['clientWidth'],state['clientHeight'])==client(resize_subject()))(json.loads(invoke('state'))),'ordinary client Fill receipt')
  ipc(resize_identity+'lumen_window restore');wait(lambda:client(resize_subject())==original_client and (lambda state:(state['clientWidth'],state['clientHeight'])==original_client)(json.loads(invoke('state'))),'ordinary client restore receipt')
  invoke('resizeClient',520,360);wait(lambda:client(resize_subject())==(520,360),'ordinary client self-resize after restore acknowledgement');records.append(dict(case='client self-resize remains supported after restore acknowledgement',window=resize_subject()))
  invoke('showClient','false');wait(lambda:resize_subject() is None,'ordinary client unmaps')
  before=reset()

 pointer=launch([a.pointer,'pointer','move','0','0'],'pointer.log',stdin=sp.PIPE,text=True)
 for mode in ('none','zoom','fill','minimize'):
  reset();ipc('lumen_window titlebar-action '+mode);box=subject();deco=box['deco_rect'];assert deco['height']>0;ipc('seat seat0 cursor set '+str(box['rect']['x']+180)+' '+str(box['rect']['y']-deco['height']//2));pointer.stdin.write('m 0 0\np\nr\np\nr\n');pointer.stdin.flush()
  if mode=='none':time.sleep(.3);assert saved(subject())==before
  elif mode=='zoom':wait(lambda:subject()['lumen_state'].get('zoomed') and client(subject())==(640,440),'titlebar Zoom')
  elif mode=='fill':wait(lambda:subject()['lumen_state'].get('filled') and not subject()['lumen_state'].get('zoomed'),'titlebar Fill')
  else:
   wait(lambda:subject()['lumen_state'].get('minimised'),'titlebar minimize');ipc(identity+'scratchpad show');wait(lambda:subject()['visible'],'restore minimized subject')
  records.append(dict(case='physical titlebar double-click '+mode,after=subject()));time.sleep(.35)
 reset();action('preferred-size 2000 1600');ipc(identity+'move container to output HEADLESS-2');action('zoom');small=wait(lambda:subject() if subject()['lumen_state'].get('zoomed') else None,'small output Zoom');assert client(small)[0]<=800 and client(small)[1]<600;records.append(dict(case='oversized preferred client fits smaller output',window=small));action('restore')
 ipc('output HEADLESS-2 resolution 650x480');wait(lambda:next(row for row in json.loads(call('swaymsg','-r','-t','get_outputs')) if row['name']=='HEADLESS-2')['rect']['width']==650,'actual output shrink');action('zoom');shrunk=wait(lambda:subject() if subject()['lumen_state'].get('zoomed') and client(subject())[0]<=650 and client(subject())[1]<480 else None,'Zoom respects changed output size');records.append(dict(case='actual output shrink before Zoom',window=shrunk));action('restore')
 ipc(identity+'move container to output HEADLESS-1');reset();action('preferred-size 640 440')
 invoke('minSize',700,500);time.sleep(.15);action('zoom');wait(lambda:client(subject())==(700,500),'protocol minimum clamps preferred client');records.append(dict(case='actual protocol minimum constraints',window=subject()));action('restore');invoke('minSize',1,1)
 # A captured title no longer belonging to this owner is rejected, never re-resolved.
 invoke('title','Renamed Zoom Fixture');wait(lambda:any(row.get('name')=='Renamed Zoom Fixture' for row in nodes(tree())),'owner title change')
 stale=register(cid,'Preferred Zoom Fixture');assert 'error' in stale,stale
 renamed=next(row for row in nodes(tree()) if row.get('id')==cid);assert renamed['name']=='Renamed Zoom Fixture';records.append(dict(case='captured helper rejects changed title',response=stale,window=renamed))
 invoke('title','Preferred Zoom Fixture');wait(subject,'restored title')
 # Invalid metadata never silently truncates or alters the existing preference.
 for value in ('0 440','640 0','32769 440','640x 440'):
  failure=raw(identity+'lumen_window preferred-size '+value);assert not all(row.get('success') for row in failure),failure
 records.append(dict(case='invalid preferred dimensions reject'))
 invoke('minSize',1500,1000);wait(lambda:not subject()['lumen_state'].get('preferred_zoom'),'minimum exceeds usable workspace capability');failure=raw(identity+'lumen_window zoom');assert not all(row.get('success') for row in failure),failure;records.append(dict(case='minimum beyond workspace truthfully disables Zoom',response=failure));invoke('minSize',1,1)
 if a.effects:
  log=(out/'compositor.log').read_text();assert all(text in log for text in ('Creating scenefx FX renderer','GL renderer: llvmpipe','FX RENDERER: Shaders Initialized Successfully','Trying udmabuf allocator','Created GL FBO for buffer'))
 (out/'result.json').write_text(json.dumps(dict(passed=True,source=call('git','-C',str(Path(__file__).resolve().parents[2]),'rev-parse','HEAD'),helper=str(Path(shutil.which('lumen-window-geometry',path=env['PATH'])).resolve()),checks=records,launcher=str(a.launcher),compositor=a.sway,effects=a.effects,fixtureSha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),limitations=['Same-user preferred-size registration is a presentation contract, not a security boundary','Physical titlebar gestures cover each preference once; no third-party preferred-size inference']),indent=2))
finally:
 for proc in reversed(processes):
  if proc.poll() is None:
   os.killpg(proc.pid,signal.SIGTERM)
   try:proc.wait(timeout=5)
   except sp.TimeoutExpired:os.killpg(proc.pid,signal.SIGKILL);proc.wait(timeout=5)
 close_reaped_pipes(processes);private.cleanup()

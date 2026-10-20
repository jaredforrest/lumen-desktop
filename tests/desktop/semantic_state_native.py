"""Actual keyboard state distinctions with visible semantic status in a private session."""
import argparse,hashlib,json,os,re,shutil,signal,subprocess as sp,sys,time
from collections import Counter
from pathlib import Path
from application_fixture.isolation import run
from application_fixture.keyboard import PhysicalKeyboard
from application_fixture.provenance import FrozenSource,generated
from private_native_bus import start_private_bus
from fixture_resources import close_reaped_pipes
p=argparse.ArgumentParser();p.add_argument('--pointer-tool',type=Path,required=True);p.add_argument('--keyboard',type=Path,required=True);p.add_argument('--launcher',type=Path,required=True);p.add_argument('--sway',type=Path,required=True);p.add_argument('--font-config',type=Path,required=True);p.add_argument('--software-driver',type=Path,required=True);p.add_argument('--output',type=Path,required=True);p.add_argument('--effects',action='store_true');p.add_argument('--inside',action='store_true');a=p.parse_args();repo=Path(__file__).resolve().parents[2]
for name in ('launcher','sway','font_config','software_driver','keyboard','pointer_tool'):
 value=getattr(a,name).resolve();assert str(value).startswith('/nix/store/') and value.exists();setattr(a,name,value)
if not a.inside:
 a.output=a.output.resolve();a.output.mkdir(mode=0o700,parents=True,exist_ok=False)
 runtime=Path(sys.executable).resolve();frozen=FrozenSource(repo,__file__,[a.launcher,a.sway,a.font_config,a.keyboard,a.pointer_tool,runtime]);prefix=re.split(r'\n[ \t]*exec quickshell ',a.launcher.read_text(),maxsplit=1)[0];env=dict(v.decode().split('=',1) for v in sp.check_output(['bash','-c',prefix+'\nexec env -0']).split(b'\0') if b'=' in v)
 args=[str(runtime),'/source/tests/desktop/semantic_state_native.py','--inside','--output','/artifacts']+sum((['--'+name.replace('_','-'),str(getattr(a,name))] for name in ('launcher','sway','font_config','software_driver','keyboard','pointer_tool')),[])+(['--effects'] if a.effects else [])
 code=1
 try:code=run(repo,a.output,args,env,software_graphics=a.effects)
 finally:frozen.finish(a.output,code)
 raise SystemExit(code)
assert repo==Path('/source') and os.getuid()!=0 and os.environ['HOME']=='/home/fixture' and not Path('/.old-root').exists();assert not any((Path('/dev')/name).exists() for name in ('dri','snd','video0','tty'))
out=a.output;processes=[];env=dict(os.environ,WLR_BACKENDS='headless',WLR_HEADLESS_OUTPUTS='1',WLR_RENDERER='gles2' if a.effects else 'pixman',WLR_RENDERER_FORCE_SOFTWARE='1' if a.effects else '0',WLR_RENDERER_ALLOW_SOFTWARE='1',WLR_LIBINPUT_NO_DEVICES='1',LIBGL_ALWAYS_SOFTWARE='1',MESA_LOADER_DRIVER_OVERRIDE='llvmpipe',LIBGL_DRIVERS_PATH=str(a.software_driver/'lib/dri'),__EGL_VENDOR_LIBRARY_FILENAMES=str(a.software_driver/'share/glvnd/egl_vendor.d/50_mesa.json'),FONTCONFIG_FILE=str(a.font_config),QT_QPA_PLATFORM='wayland',QT_QUICK_BACKEND='software',QT_QPA_PLATFORMTHEME='generic',QT_NO_XDG_DESKTOP_PORTAL='1',QT_ACCESSIBILITY='0',NO_AT_BRIDGE='1',GTK_A11Y='none',LUMEN_UNIFIED_OWNED_HEADERS='1',LUMEN_NATIVE_WINDOW_CONTROLS='0',LUMEN_NATIVE_WINDOW_ACTIONS='0',LUMEN_LAUNCH_SCOPES='0',LUMEN_NEARBY_SHARING_PROVIDER='',GLOBAL_MENU_PYTHON=sys.executable,LUMEN_COMPOSITOR_EFFECTS='1' if a.effects else '0')
for key in ('SWAYSOCK','WAYLAND_DISPLAY','DISPLAY','AT_SPI_BUS_ADDRESS'):env.pop(key,None)
assets=out/'assets';shutil.copytree(repo/'res/lumen-desktop',assets);palette=json.loads((assets/'appearance/palette.json').read_text());appearance=out/'appearance.json';env['LUMEN_APPEARANCE_STATE']=str(appearance)
def theme(mode,contrast):
 pending=appearance.with_suffix('.pending');pending.write_text(json.dumps(dict(font=palette['font'],palette=palette[mode],preferences=dict(reduceMotion=True,reduceTransparency=True,increaseContrast=contrast))));pending.replace(appearance)
theme('light',False);qml=out/'semantic.qml';qml.write_text('''import QtQuick
import QtQuick.Controls
import Quickshell
import Quickshell.Io
import "assets/shell" as D
ShellRoot {id:root
 property int deleted:0
 property var pointerReceipts:[]
 FloatingWindow {id:window;visible:true;title:"Private semantic states";implicitWidth:600;implicitHeight:360;color:D.Theme.colors.background
  MouseArea {anchors.fill:parent;z:-1;onClicked:mouse=>root.pointerReceipts=root.pointerReceipts.concat({x:mouse.x,y:mouse.y})}
  Column {id:frame;x:20;y:20;width:560;spacing:16
   D.StatusMessage {id:status;width:parent.width;text:"The request returned a result";severity:"error"}
   D.SettingsToggle {id:toggle;width:parent.width;text:"Private permission";checked:false}
   D.ControlTile {id:radio;width:parent.width;label:"Private connection";symbol:"wifi";toggle:true;unavailable:false;active:false;subtitle:active?"Connected":"Disconnected";onClicked:active=!active}
   D.DialogButton {id:remove;width:parent.width;text:"Delete private item";destructive:true;enabled:false;defaultAction:false;onClicked:root.deleted++}
  }
 }
 function walk(item){let rows=[item];for(const child of item.children || [])rows=rows.concat(walk(child));return rows;}
 IpcHandler {target:"semantic"
  function focus(name:string):bool{const item=name==="toggle"?toggle:radio;item.forceActiveFocus();return item.activeFocus;}
  function severity(value:string):void{status.severity=value;}
  function availability(value:bool):void{radio.unavailable=!value;}
  function state():string {const track=root.walk(toggle).find(item=>item.width===32 && item.height===20),thumb=track?Array.from(track.children).find(item=>item.width===16):null;return JSON.stringify({dark:D.Theme.dark,contrast:D.Theme.increaseContrast,visible:window.visible,checked:toggle.checked,thumb:thumb?thumb.x:null,toggleFocus:toggle.activeFocus,radioFocus:radio.activeFocus,radioEnabled:radio.enabled,pointerReceipts:root.pointerReceipts,radioRect:{x:radio.mapToItem(window.contentItem,0,0).x,y:radio.mapToItem(window.contentItem,0,0).y,width:radio.width,height:radio.height},active:radio.active,unavailable:radio.unavailable,radioName:radio.Accessible.name,deleted:root.deleted,statusName:status.Accessible.name,severity:status.severity,statusTexts:root.walk(status).filter(item=>item.visible && item.text!==undefined).map(item=>item.text),rect:{width:window.width,height:window.height},messageWidth:status.width,messageHeight:status.height});}
 }
}''')
def launch(command,name,**kwargs):
 with (out/name).open('w') as log:proc=sp.Popen(command,env=env,stdout=log,stderr=sp.STDOUT,start_new_session=True,**kwargs)
 processes.append(proc);return proc
def call(*args):return sp.check_output(list(map(str,args)),env=env,text=True,timeout=6).strip()
def ipc(method,*args):return call('quickshell','-p',qml,'ipc','call','semantic',method,*args)
def state():return json.loads(ipc('state'))
def tree():return json.loads(call(a.sway.parent/'swaymsg','-r','-t','get_tree'))
def nodes(node):
 yield node
 for child in node.get('nodes',[])+node.get('floating_nodes',[]):yield from nodes(child)
def wait(fn):
 end=time.monotonic()+15
 while time.monotonic()<end:
  try:value=fn()
  except (sp.SubprocessError,OSError,ValueError):value=None
  if value:return value
  time.sleep(.04)
 (out/'timeout-tree.json').write_text(json.dumps(tree(),indent=2));raise AssertionError('Semantic input observation deadline')
try:
 start_private_bus(env,out,processes);config=out/'sway.conf';config.write_text('output HEADLESS-1 resolution 1000x700\nxwayland disable\nfocus_follows_mouse no\nfor_window [app_id="org.quickshell"] floating enable, border none\n');launch([str(a.sway),'--unsupported-gpu','-c',str(config)],'sway.log');env['SWAYSOCK']=str(wait(lambda:next(Path(env['XDG_RUNTIME_DIR']).glob('sway-ipc*.sock'),None)));env['WAYLAND_DISPLAY']=wait(lambda:next((p.name for p in Path(env['XDG_RUNTIME_DIR']).glob('wayland-*') if not p.name.endswith('.lock')),None))
 with (out/'keyboard.log').open('w') as log:keyboard=PhysicalKeyboard(a.keyboard,env,log)
 processes.append(keyboard.process);pointer=launch([str(a.pointer_tool),'pointer','move','0','0'],'pointer.log',stdin=sp.PIPE,text=True);pointer.stdin.write('m 0 0\n');pointer.stdin.flush();call(a.sway.parent/'swaymsg','seat seat0 cursor set 0 0');position=[0,0];shell=launch(['quickshell','-p',str(qml)],'shell.log');node=wait(lambda:next((n for n in nodes(tree()) if n.get('pid')==shell.pid and n.get('name')=='Private semantic states' and n.get('visible')),None));cid=node['id'];wait(lambda:state()['visible']);checks=[]
 for mode,contrast in (('light',False),('light',True),('dark',False),('dark',True)):
  theme(mode,contrast);wait(lambda:state()['dark']==(mode=='dark') and state()['contrast']==contrast);initial=state();assert any(n.get('id')==cid and n.get('visible') for n in nodes(tree()))
  for severity in ('error','success','warning'):
   ipc('severity',severity);wait(lambda:state()['severity']==severity and severity.capitalize()+':' in state()['statusTexts']);assert state()['statusName']==severity.capitalize()+': The request returned a result'
  assert ipc('focus','toggle')=='true';before=state()['checked'];keyboard.key('-k','space');wait(lambda:state()['checked']!=before);assert state()['thumb']==(14 if not before else 2)
  assert ipc('focus','radio')=='true';before=state()['active'];keyboard.key('-k','Return');wait(lambda:state()['active']!=before);assert ('Connected' if not before else 'Disconnected') in state()['radioName']
  ipc('availability','false');wait(lambda:state()['unavailable'] and not state()['radioFocus'] and not state()['radioEnabled']);before=state()['active'];keyboard.key('-k','Return');assert state()['active']==before and state()['deleted']==0 and 'unavailable' in state()['radioName'];keyboard.key('-k','Tab');wait(lambda:state()['toggleFocus'])
  node=next(n for n in nodes(tree()) if n.get('id')==cid);r=state()['radioRect'];x=r['x']+r['width']/2;y=r['y']+r['height']/2;count=len(state()['pointerReceipts']);target=[node['rect']['x']+node['window_rect']['x']+x,node['rect']['y']+node['window_rect']['y']+y];pointer.stdin.write('m %.5f %.5f\np\nr\n'%(target[0]-position[0],target[1]-position[1]));pointer.stdin.flush();position[:]=target;wait(lambda:len(state()['pointerReceipts'])==count+1);receipt=state()['pointerReceipts'][-1];assert abs(receipt['x']-x)<2 and abs(receipt['y']-y)<2 and state()['active']==before and state()['deleted']==0;ipc('availability','true')
  snapshot=state();assert snapshot['rect']==initial['rect'];call('grim',out/(mode+'-'+str(contrast)+'.png'));checks.append(dict(mode=mode,contrast=contrast,state=snapshot))
 logs=(out/'shell.log').read_text();assert not any(marker in logs for marker in ('ReferenceError:','TypeError:','Binding loop','Failed to load configuration','Cannot assign','WARN','ERROR')),logs
 (out/'result.json').write_text(json.dumps(dict(passed=True,checks=checks,scope='Actual private keyboard switch/radio state transitions, unavailable refusal and unchanged native window across four palette/contrast cases; severity has visible labels and unchanged AT name. No external actions.'),indent=2));print(json.dumps(dict(passed=True,checks=len(checks))))
finally:
 generated(out,out)
 for proc in reversed(processes):
  if proc.poll() is None:
   try:os.killpg(proc.pid,signal.SIGTERM)
   except ProcessLookupError:pass
 for proc in reversed(processes):
  try:proc.wait(timeout=5)
  except sp.TimeoutExpired:os.killpg(proc.pid,signal.SIGKILL);proc.wait(timeout=5)
 close_reaped_pipes(processes)

# Qt may emit warnings while disconnecting; the final gate runs after every child is reaped.
logs=(out/'shell.log').read_text();assert not any(marker in logs for marker in ('WARN','ERROR')),logs

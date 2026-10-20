"""Native mixed-output scale geometry and positive input acceptance, privately."""
import argparse
import json
import os
from pathlib import Path
import re
import shutil
import signal
import hashlib
import subprocess as sp
import tempfile
import time
from PIL import Image
from fixture_resources import close_reaped_pipes
from private_native_bus import start_private_bus

p=argparse.ArgumentParser()
p.add_argument('--launcher',type=Path,required=True)
p.add_argument('--sway',required=True)
p.add_argument('--output',type=Path,required=True)
p.add_argument('--effects',action='store_true')
p.add_argument('--pointer-tool',default='wlrctl')
a=p.parse_args()
out=a.output.resolve();out.mkdir(parents=True,exist_ok=False)
repo=Path(__file__).resolve().parents[2]
processes=[]
private=tempfile.TemporaryDirectory(prefix='lumen-mixed-scale-')
env=dict(os.environ,HOME=private.name,XDG_RUNTIME_DIR=private.name)
for key in ('CONFIG','STATE','CACHE','DATA'):env['XDG_'+key+'_HOME']=str(Path(private.name)/key.lower())
for key in ('DISPLAY','WAYLAND_DISPLAY','SWAYSOCK','DBUS_SESSION_BUS_ADDRESS','AT_SPI_BUS_ADDRESS'):env.pop(key,None)
prefix=re.split(r'\n[ \t]*exec quickshell ',a.launcher.read_text(),maxsplit=1)[0]
raw=sp.check_output(['bash','-c',prefix+'\nexec env -0'],env=env)
env.update(dict(value.decode().split('=',1) for value in raw.split(b'\0') if b'=' in value))
env.update(WLR_BACKENDS='headless',WLR_HEADLESS_OUTPUTS='2',WLR_RENDERER='gles2' if a.effects else 'pixman',WLR_RENDERER_FORCE_SOFTWARE='1' if a.effects else '0',WLR_RENDERER_ALLOW_SOFTWARE='1',LIBGL_ALWAYS_SOFTWARE='1',WLR_LIBINPUT_NO_DEVICES='1',QT_QPA_PLATFORM='wayland',QT_QPA_PLATFORMTHEME='generic',QT_QUICK_BACKEND='software',QT_ACCESSIBILITY='0',QT_LINUX_ACCESSIBILITY_ALWAYS_ON='0',NO_AT_BRIDGE='1',GTK_A11Y='none',LUMEN_NATIVE_WINDOW_CONTROLS='1',LUMEN_NATIVE_WINDOW_ACTIONS='0',LUMEN_UNIFIED_OWNED_HEADERS='1')
assets=out/'assets';shutil.copytree(repo/'res/lumen-desktop',assets)
# Seed the private watcher file before constructing Theme: no missing-file warning.
appearance=out/'appearance.json'
appearance.write_text(json.dumps(dict(font='Interlude Variable',palette=dict(background='#f2f2f4',surface='#ffffff',field='#e5e5e8',text='#202023',muted='#626267',border='#c8c8ce',hover='#dedee3',accent='#007aff',selectedText='#000000'),preferences=dict(mode='light',reduceMotion=True,reduceTransparency=True))))
env['LUMEN_APPEARANCE_STATE']=str(appearance)
qml=out/'shell.qml'
qml.write_text('''import QtQuick
import Quickshell
import Quickshell.Io
import "assets/shell" as D
ShellRoot {id:fixtureRoot
 function pressedButton(object) {if(!object)return null;if(object.pressed===true)return object;for(const child of object.children || []){const found=pressedButton(child);if(found)return found;}return null;}
 function findSearch(object) {if(!object)return null;if(object.placeholderText==="Search Help")return object;for(const child of object.children || []){const found=findSearch(child);if(found)return found;}return null;}
 QtObject {id:stub;property string popupOwner:"";signal dismissPopups(string owner);function claimPopup(owner){popupOwner=owner;dismissPopups(owner);}function launchApps(){}}
 D.HelpWindow {id:help;shell:stub}
 FloatingWindow {id:probe;title:"Scale specimen";visible:false;implicitWidth:520;implicitHeight:360;color:D.Theme.colors.background
  property int actions:0
  D.AppIcon {id:icon;x:24;y:24;width:48;height:48;iconSource:Qt.resolvedUrl("assets/shell/reference-icons/files.svg")}
  D.PanelLabel {id:label;x:88;y:28;text:"0123456789 12:34 — café Ångström"}
  D.DialogButton {id:action;x:24;y:100;text:"Recorded action";onClicked:probe.actions++}
 }
 IpcHandler {target:"scale"
  function openWindow(kind:string):void {help.visible=kind==="owned";probe.visible=kind==="native";}
  function state():string {const search=fixtureRoot.findSearch(help.contentItem);const position=search?search.mapToItem(help.contentItem,0,0):Qt.point(0,0);const held=fixtureRoot.pressedButton(help.contentItem);return JSON.stringify({held:!!held,heldHovered:held?held.hovered:false,search:search?{x:position.x,y:position.y,width:search.width,height:search.height,focused:search.activeFocus}:null,actions:probe.actions,query:help.query,icon:{x:icon.x,y:icon.y,width:icon.width,height:icon.height},label:{x:label.x,y:label.y,width:label.width,height:label.height},action:{x:action.x,y:action.y,width:action.width,height:action.height},help:{width:help.width,height:help.height}});}
 }
}''')
config=out/'sway.conf'
config.write_text('font sans 10\noutput * resolution 1600x1200\noutput HEADLESS-1 position 0 0 scale 1\noutput HEADLESS-2 position 1600 0 scale 1.25\noutput * bg #29445b solid_color\ndefault_border pixel 0\ndefault_floating_border pixel 0\nfor_window [all] floating enable\nfor_window [title="^Scale specimen$"] border normal 1\n')
if a.effects:
 with config.open('a') as stream:stream.write('corner_radius 12\nshadows enable\nanimation_duration_ms 0\n')
def launch(argv,name):
 with (out/name).open('w') as log:process=sp.Popen(argv,env=env,stdout=log,stderr=sp.STDOUT,start_new_session=True)
 processes.append(process);return process

def call(*argv):return sp.check_output(argv,env=env,text=True,timeout=10)
def ipc(command):
 result=json.loads(call('swaymsg',command));assert all(row.get('success') for row in result),result

def wait(fn):
 deadline=time.monotonic()+15
 last=None
 while time.monotonic()<deadline:
  try:
   value=fn()
   if value:return value
  except (OSError,ValueError,sp.SubprocessError) as error:last=str(error)
  time.sleep(.05)
 try:(out/'failure-state.json').write_text(json.dumps(dict(state=state(),tree=json.loads(call('swaymsg','-t','get_tree','-r'))),indent=2))
 except (NameError,OSError,ValueError,sp.SubprocessError):pass
 raise AssertionError(('Bounded native condition failed',last))

def nodes(node):
 yield node
 for child in node.get('nodes',[])+node.get('floating_nodes',[]):yield from nodes(child)
def window(title):return next((node for node in nodes(json.loads(call('swaymsg','-t','get_tree','-r'))) if node.get('name')==title and node.get('app_id')),None)
def outputs():return {row['name']:row for row in json.loads(call('swaymsg','-t','get_outputs','-r')) if row['active']}
def state():return json.loads(call('quickshell','-p',str(qml),'ipc','call','scale','state'))
def click(x,y):
 ipc(f'seat seat0 cursor set {round(x)} {round(y)}')
 pointer.stdin.write('m 0 0\np\nr\n');pointer.stdin.flush()

records=[]
try:
 start_private_bus(env,out,processes)
 launch([a.sway,'-d','--unsupported-gpu','-c',str(config)],'compositor.log')
 wait(lambda:list(Path(private.name).glob('sway-ipc.*.sock')))
 env['SWAYSOCK']=str(next(Path(private.name).glob('sway-ipc.*.sock')))
 wait(lambda:len(outputs())==2)
 env['WAYLAND_DISPLAY']=next(path.name for path in Path(private.name).glob('wayland-*') if not path.name.endswith('.lock'))
 ipc('lumen_window margins disable')
 launch(['quickshell','-p',str(qml)],'shell.log');wait(state)
 with (out/'pointer.log').open('w') as log:pointer=sp.Popen([a.pointer_tool,'pointer','move','0','0'],env=env,stdin=sp.PIPE,stdout=log,stderr=sp.STDOUT,text=True,start_new_session=True)
 processes.append(pointer)
 launch(['wtype','-s','60000'],'keyboard.log')
 pointer.stdin.write('m 0 0\n');pointer.stdin.flush()
 plan=[('HEADLESS-1',1),('HEADLESS-2',1.25),('HEADLESS-2',1.5),('HEADLESS-2',2),('HEADLESS-1',1),('HEADLESS-1',1.25),('HEADLESS-1',1.5),('HEADLESS-2',2),('HEADLESS-2',1.25),('HEADLESS-1',1.5)]
 for kind,title in [('native','Scale specimen'),('owned','Desktop Help')]:
  call('quickshell','-p',str(qml),'ipc','call','scale','openWindow',kind)
  initial=wait(lambda:window(title) if window(title) and (kind!='native' or window(title)['deco_rect']['height']==32) else None);identity=initial['id'];baseline=None
  for index,(output,scale) in enumerate(plan):
   ipc(f'output {output} scale {scale}')
   ipc(f'[con_id={identity}] move container to output {output}')
   ipc(f'[con_id={identity}] move position center')
   wait(lambda:window(title) and window(title)['id']==identity)
   # IPC command completion does not imply a Qt frame or keyboard focus event.
   ipc(f'[con_id={identity}] focus')
   current=window(title)
   assert current['window_rect']['width']==initial['window_rect']['width'] and current['window_rect']['height']==initial['window_rect']['height'],(kind,index,initial,current)
   before=state();logical={key:before[key] for key in ('icon','label','action','help')}
   if baseline is None:baseline=logical
   assert logical==baseline,(kind,index,baseline,logical)
   display=outputs()[output];rect=display['rect'];box=current['rect']
   assert rect['x']<=box['x'] and box['x']+box['width']<=rect['x']+rect['width'],(display,current)
   assert rect['y']<=box['y'] and box['y']+box['height']<=rect['y']+rect['height'],(display,current)
   image=out/f'{kind}-{index}-{scale}.png'
   def painted():
    call('grim','-o',output,'-s',str(scale),str(image))
    with Image.open(image) as source:
     capture=source.convert('RGB');physical=capture.size
     assert abs(physical[0]-rect['width']*scale)<=1 and abs(physical[1]-rect['height']*scale)<=1,(physical,rect,scale)
     mode=display['current_mode']
     assert abs(physical[0]-mode['width'])<=1 and abs(physical[1]-mode['height'])<=1,(physical,mode,scale)
     def crop(local):
      x,y,w,h=local
      return capture.crop(tuple(round(value*scale) for value in (box['x']-rect['x']+x,box['y']-rect['y']+y,box['x']-rect['x']+x+w,box['y']-rect['y']+y+h)))
     # These blank body coordinates are deliberately away from labels/edges.
     # A logical state alone cannot prove the new output has painted the window.
     body=crop((300,250,20,20) if kind=='native' else (500,20,20,20))
     body_pixels=list(body.get_flattened_data())
     if not body_pixels or sum(max(pixel)-min(pixel)<12 and min(pixel)>200 for pixel in body_pixels)<len(body_pixels)*.95:return False
     regions={}
     if kind=='native':
      controls=[]
      for offset,expected in [(20,(255,95,87)),(40,(254,188,46)),(60,(40,200,64))]:
       point=(round((box['x']-rect['x']+offset)*scale),round((box['y']-rect['y']-16)*scale))
       color=capture.getpixel(point)
       if max(abs(color[channel]-expected[channel]) for channel in range(3))>1:return False
       controls.append(dict(pixel=point,rgb=color))
      regions['nativeControls']=controls
     if kind=='native':
      for name in ('label','icon'):
       target=before[name];region=crop((target['x'],target['y'],target['width'],target['height']))
       pixels=list(region.get_flattened_data())
       count=sum(min(pixel)<180 for pixel in pixels)
       if count<10*scale*scale:return False
       regions[name]=dict(physical=region.size,paintedPixels=count)
     # Retain actual corner/edge samples in physical pixel coordinates.
     # Their shape is compositor-specific; no invented radius is asserted.
     samples={}
     for name,(x,y) in dict(topLeft=(0,0),innerCorner=(10,10),leftInterior=(4,180),rightInterior=(box['width']-5,180)).items():
      px=round((box['x']-rect['x']+x)*scale);py=round((box['y']-rect['y']+y)*scale)
      assert 0<=px<capture.width and 0<=py<capture.height
      samples[name]=dict(pixel=[px,py],rgb=capture.getpixel((px,py)))
     return dict(physical=physical,paint=regions,edgeSamples=samples)
   paint=wait(painted);physical=paint['physical']
   if kind=='native':
    target=before['action'];click(box['x']+target['x']+target['width']/2,box['y']+target['y']+target['height']/2)
    wait(lambda:state()['actions']==before['actions']+1)
    # A positive native action proves the frame hit target at every scale.
    click(box['x']+60,box['y']-16)
    wait(lambda:window(title)['fullscreen_mode']!=0)
    ipc(f'[con_id={identity}] lumen_window fullscreen')
    wait(lambda:window(title)['fullscreen_mode']==0 and window(title)['rect']==current['rect'])
   else:
    target=before['search'];assert target,before
    click(box['x']+target['x']+target['width']/2,box['y']+target['y']+target['height']/2)
    wait(lambda:state()['search']['focused'] and window(title)['focused'])
    call('wtype','-k','End','-k','a');wait(lambda:state()['query']==before['query']+'a')
   record=dict(kind=kind,index=index,scale=scale,output=display,window=current,physical=physical,logical=logical,paint=paint,state=state(),capture=str(image))
   records.append(record);(out/'last-state.json').write_text(json.dumps(record,indent=2))
  if kind=='owned':
   # Owned close cancellation must not dismiss while crossing off the target.
   box=window(title)['rect'];ipc(f'seat seat0 cursor set {box["x"]+20} {box["y"]+24}')
   pointer.stdin.write('m 0 0\np\n');pointer.stdin.flush()
   wait(lambda:state()['held'])
   pointer.stdin.write('m 100 0\n');pointer.stdin.flush()
   pointer.stdin.write('m 0 0\nr\n');pointer.stdin.flush()
   wait(lambda:not state()['held'])
   assert window(title) is not None
   click(box['x']+20,box['y']+24);wait(lambda:window(title) is None)
 (out/'result.json').write_text(json.dumps(dict(passed=True,cases=records,source=call('git','-C',str(repo),'rev-parse','HEAD').strip(),launcher=str(a.launcher),compositor=a.sway,fixtureSha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),assetsSha256={str(path.relative_to(assets)):hashlib.sha256(path.read_bytes()).hexdigest() for path in sorted(assets.rglob('*')) if path.is_file()},scope='Qt software content on native mixed-scale compositor outputs; engineering geometry/input invariants'),indent=2))
finally:
 (out/'process-status.json').write_text(json.dumps([dict(argv=process.args,returncode=process.poll()) for process in processes],indent=2))
 for process in reversed(processes):
  if process.poll() is None:
   os.killpg(process.pid,signal.SIGTERM)
   try:process.wait(timeout=5)
   except sp.TimeoutExpired:os.killpg(process.pid,signal.SIGKILL);process.wait(timeout=5)
 close_reaped_pipes(processes)
 private.cleanup()

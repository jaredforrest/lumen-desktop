"""Paint vivid content at declared owned content boundaries in a private session."""
import argparse
import json
import os
from pathlib import Path
import re
import shutil
import signal
import hashlib
import math
import subprocess as sp
import tempfile
import time
from PIL import Image
from fixture_resources import close_reaped_pipes
from frame_boundary import outside_frame, raster_origin
from private_native_bus import start_private_bus

p=argparse.ArgumentParser()
p.add_argument('--launcher',type=Path,required=True)
p.add_argument('--sway',required=True)
p.add_argument('--output',type=Path,required=True)
p.add_argument('--effects',action='store_true')
p.add_argument('--native-frame',choices=['on','off'],required=True)
p.add_argument('--content-backend',choices=['software','opengl'],required=True)
p.add_argument('--pointer-tool',default='wlrctl')
a=p.parse_args()
out=a.output.resolve();out.mkdir(parents=True,exist_ok=False)
repo=Path(__file__).resolve().parents[2]
processes=[]
private=tempfile.TemporaryDirectory(prefix='lumen-clipping-')
env=dict(os.environ,HOME=private.name,XDG_RUNTIME_DIR=private.name)
for key in ('CONFIG','STATE','CACHE','DATA'):env['XDG_'+key+'_HOME']=str(Path(private.name)/key.lower())
for key in ('DISPLAY','WAYLAND_DISPLAY','SWAYSOCK','DBUS_SESSION_BUS_ADDRESS','AT_SPI_BUS_ADDRESS'):env.pop(key,None)
prefix=re.split(r'\n[ \t]*exec quickshell ',a.launcher.read_text(),maxsplit=1)[0]
raw=sp.check_output(['bash','-c',prefix+'\nexec env -0'],env=env)
env.update(dict(value.decode().split('=',1) for value in raw.split(b'\0') if b'=' in value))
env.update(WLR_BACKENDS='headless',WLR_HEADLESS_OUTPUTS='2',WLR_RENDERER='gles2' if a.effects else 'pixman',WLR_RENDERER_FORCE_SOFTWARE='1' if a.effects else '0',WLR_RENDERER_ALLOW_SOFTWARE='1',LIBGL_ALWAYS_SOFTWARE='1',WLR_LIBINPUT_NO_DEVICES='1',QT_QPA_PLATFORM='wayland',QT_QPA_PLATFORMTHEME='generic',QT_QUICK_BACKEND='software',QT_ACCESSIBILITY='0',QT_LINUX_ACCESSIBILITY_ALWAYS_ON='0',NO_AT_BRIDGE='1',GTK_A11Y='none',LUMEN_NATIVE_WINDOW_CONTROLS='1',LUMEN_NATIVE_WINDOW_ACTIONS='0',LUMEN_UNIFIED_OWNED_HEADERS='1')
env['LUMEN_UNIFIED_OWNED_HEADERS']='0' if a.native_frame=='on' else '1'
if a.content_backend=='opengl':
 env.pop('QT_QUICK_BACKEND',None);env['QSG_RHI_BACKEND']='opengl'
assets=out/'assets';shutil.copytree(repo/'res/lumen-desktop',assets)
# Seed the private watcher file before constructing Theme: no missing-file warning.
appearance=out/'appearance.json'
appearance.write_text(json.dumps(dict(font='Interlude Variable',palette=dict(background='#f2f2f4',surface='#ffffff',field='#e5e5e8',text='#202023',muted='#626267',border='#c8c8ce',hover='#dedee3',accent='#007aff',selectedText='#000000'),preferences=dict(mode='light',reduceMotion=True,reduceTransparency=True))))
env['LUMEN_APPEARANCE_STATE']=str(appearance)
qml=out/'shell.qml'
qml.write_text('''import QtQuick
import QtQuick.Controls
import QtQuick.Window
import Quickshell
import Quickshell.Io
import "assets/shell" as D
ShellRoot {id:fixture
 QtObject {id:service
  property var menuBarPreferences:D.Theme.menuBarPreferences
  property var preferences:D.Theme.preferences
  property var desktopPreferences:D.Theme.desktopPreferences
  property var windowsPreferences:D.Theme.windowsPreferences
  property var notificationsPreferences:({preview:"always"})
  property var searchPreferences:({disabledProviders:[]})
  property var appearance:({})
  property var system:({system:"Fixture",desktop:"Fixture"})
  property bool available:true;property bool busy:false;property bool applying:false
  property string error:"";property string warning:""
  property int edits:0
  function refresh(){}
  function setPreference(key,value){edits++;}
  function setMenuBarPreference(key,value){}
  function setDesktopPreference(key,value){}
  function setWindowPreference(key,value){}
  function setNotificationPreference(key,value){}
  function setSearchPreference(key,value){}
 }
 QtObject {id:stub
  property string volume:"Volume: 0.5"
  property var audioState:({available:false,muted:false,outputs:[],inputs:[],streams:[],inputAvailable:false,sink:null,source:null})
  property var notifications:({dnd:false,available:false,applications:[],policies:{},history:[],error:""})
  property var search:({available:false,scanning:false,indexed:0})
  property var systemState:({});property var vpn:({})
 }
 FloatingWindow {id:aux;visible:false;title:"System Settings";implicitWidth:1200;implicitHeight:900;minimumSize:Qt.size(200,100);color:"#7755aa"}
 D.WindowGeometry {id:auxGeometry;ownerWindow:aux}
 D.SettingsWindow {id:settings;service:service;shell:stub;visible:true;page:"Accessibility"}
 property var surface:null
 property var declaredMask:null
 property var sentinel:null
 property var realHover:null
 Component {id:viewportHover;HoverHandler {}}
 function visit(item,fn){if(fn(item))return item;for(const child of item.children || []){const found=visit(child,fn);if(found)return found;}return null;}
 function rect(item){const point=item.mapToItem(settings.contentItem,0,0);return {x:point.x,y:point.y,width:item.width,height:item.height};}
 Component {id:edgeContent
  Flickable {id:scroll;property bool pointerInside:hover.hovered;property point pointerPosition:hover.point.position;HoverHandler {id:hover}
   anchors.fill:parent;interactive:false;contentWidth:width*3;contentHeight:height*3;clip:true;boundsBehavior:Flickable.StopAtBounds
   Repeater {model:400
    Rectangle {required property int index;x:(index%20)*128;y:Math.floor(index/20)*128;width:128;height:128;color:["#ff00ff","#00ff00","#00ffff"][(index+Math.floor(index/20)*2)%3]}
   }
  }
 }
 function prepare(){
  const controls=visit(settings.contentItem,item=>item.windowName==="System Settings");
  if(!controls)return false;
  surface=controls.parent;
  declaredMask=visit(settings.contentItem,item=>item!==surface && item.radius!==undefined && item.visible===false && item.width===settings.width && item.height===settings.height);
  for(const child of surface.children || [])if((child.children || []).length>0 || child.acceptedButtons!==undefined)child.z=2;
  sentinel=edgeContent.createObject(surface);const viewport=visit(settings.contentItem,item=>item.objectName==="settings-content-scroll");realHover=viewportHover.createObject(viewport);return !!sentinel && !!realHover;
 }
 IpcHandler {target:"clipping"
  function ready():bool{return !!fixture.sentinel || fixture.prepare();}
  function scroll(bottom:bool):void {fixture.sentinel.contentX=bottom?fixture.sentinel.contentWidth-fixture.sentinel.width:0;fixture.sentinel.contentY=bottom?fixture.sentinel.contentHeight-fixture.sentinel.height:0;}
  function auxiliary(title:string,captured:int):void {aux.title=title;auxGeometry.containerId=captured;aux.visible=true;}
  function hideAuxiliary():void {aux.visible=false;}
  function fitAuxiliary():void {auxGeometry.fit(500,400);}
  function fitRepeated():void {for(let i=0;i<5;i++)settings.fitScreen();}
  function resetViewport():void {const viewport=fixture.visit(settings.contentItem,item=>item.objectName==="settings-content-scroll");viewport.cancelFlick();viewport.contentY=0;}
  function state():string {const geometry=fixture.visit(settings.contentItem,item=>item.ownerWindow===settings && item.completedRequests!==undefined);const controls=fixture.visit(settings.contentItem,item=>item.windowName==="System Settings");const close=controls?fixture.visit(controls,item=>item.index===0 && item.pressed!==undefined):null;const viewport=fixture.visit(settings.contentItem,item=>item.objectName==="settings-content-scroll");const search=fixture.visit(settings.contentItem,item=>item.placeholderText==="Search");return JSON.stringify({auxiliary:{visible:aux.visible,width:aux.width,height:aux.height,busy:auxGeometry.busy,result:auxGeometry.result,error:auxGeometry.error,completed:auxGeometry.completedRequests},fit:geometry?{busy:geometry.busy,result:geometry.result,error:geometry.error,completed:geometry.completedRequests}:null,screen:settings.screen?settings.screen.name:"",closeControl:close?fixture.rect(close):null,viewport:viewport?{rect:fixture.rect(viewport),y:viewport.contentY,contentHeight:viewport.contentHeight,hovered:fixture.realHover?fixture.realHover.hovered:false,pointerX:fixture.realHover?fixture.realHover.point.position.x:0,pointerY:fixture.realHover?fixture.realHover.point.position.y:0}:null,visible:settings.visible,width:settings.width,height:settings.height,graphicsApi:settings.contentItem.GraphicsInfo.api,software:settings.contentItem.GraphicsInfo.api===GraphicsInfo.Software,native:D.Theme.nativeWindowFrame,maskActive:fixture.surface?fixture.surface.layer.enabled:false,maskRadius:fixture.declaredMask?fixture.declaredMask.radius:null,surface:fixture.surface?fixture.rect(fixture.surface):null,sentinel:fixture.sentinel?fixture.rect(fixture.sentinel):null,scroll:fixture.sentinel?{x:fixture.sentinel.contentX,y:fixture.sentinel.contentY,hovered:fixture.sentinel.pointerInside,pointerX:fixture.sentinel.pointerPosition.x,pointerY:fixture.sentinel.pointerPosition.y}:null,search:search?fixture.rect(search):null,query:search?search.text:"",searchFocused:search?search.activeFocus:false,edits:service.edits});}
 }
}''')
config=out/'sway.conf'
config.write_text('font sans 10\noutput * resolution 1920x1440\noutput HEADLESS-1 position 0 0 scale 1\noutput HEADLESS-2 resolution 1000x700 position 1920 0 scale 1.25\noutput * bg #29445b solid_color\ndefault_border pixel 0\ndefault_floating_border pixel 0\nfor_window [all] floating enable\nfor_window [title="^System Settings$"] border '+('normal 1' if a.native_frame=='on' else 'pixel 0')+'\n')
frame_radius=12 if a.effects else 0
if a.effects:
 with config.open('a') as stream:stream.write(f'corner_radius {frame_radius}\nshadows enable\nanimation_duration_ms 0\n')
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
def state():return json.loads(call('quickshell','-p',str(qml),'ipc','call','clipping','state'))
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
 ipc('focus output HEADLESS-1')
 env['WAYLAND_DISPLAY']=next(path.name for path in Path(private.name).glob('wayland-*') if not path.name.endswith('.lock'))
 ipc('lumen_window margins disable')
 launch(['quickshell','-p',str(qml)],'shell.log')
 wait(lambda:call('quickshell','-p',str(qml),'ipc','call','clipping','ready').strip()=='true')
 wait(lambda:state().get('graphicsApi') not in (None,0))
 with (out/'pointer.log').open('w') as log:pointer=sp.Popen([a.pointer_tool,'pointer','move','0','0'],env=env,stdin=sp.PIPE,stdout=log,stderr=sp.STDOUT,text=True,start_new_session=True)
 processes.append(pointer);pointer.stdin.write('m 0 0\n');pointer.stdin.flush()
 launch(['wtype','-s','60000'],'keyboard.log')
 title='System Settings';initial=wait(lambda:window(title) if window(title) and (a.native_frame=='off' or window(title)['deco_rect']['height']>0) else None);identity=initial['id']
 for scale in [1,1.25,1.5,2]:
  ipc(f'output HEADLESS-1 scale {scale}')
  for width,height in [(790,520),(520,360)]:
   before=window(title)
   paddingWidth=before['rect']['width']-before['window_rect']['width']
   paddingHeight=before['rect']['height']+before['deco_rect']['height']-before['window_rect']['height']
   ipc(f'[con_id={identity}] resize set width {width+paddingWidth} px height {height+paddingHeight} px')
   ipc(f'[con_id={identity}] move position center')
   wait(lambda:state()['width']==width and state()['height']==height)
   for bottom in [False,True]:
    call('quickshell','-p',str(qml),'ipc','call','clipping','scroll','true' if bottom else 'false')
    current=window(title);assert current['id']==identity
    status=state();assert status['software']==(a.content_backend=='software'),status
    assert status['maskActive']==(a.native_frame=='off' and a.content_backend=='opengl'),status
    display=outputs()['HEADLESS-1'];box=current['rect'];screen=display['rect'];client=current['window_rect']
    image=out/f'edge-{scale}-{width}-{int(bottom)}.png'
    surface=status['surface'];origin=(box['x']+client['x']+surface['x']-screen['x'],box['y']+client['y']+surface['y']-screen['y'])
    colors=[(255,0,255),(0,255,0),(0,255,255)]
    def painted_edges():
     call('grim','-o','HEADLESS-1','-s',str(scale),str(image))
     with Image.open(image) as source:
      capture=source.convert('RGB');samples=[]
      for x,y in [(4,surface['height']/2),(surface['width']-4,surface['height']/2),(surface['width']/2,4),(surface['width']/2,surface['height']-4)]:
       expected=colors[(math.floor((x+status['scroll']['x'])/128)+math.floor((y+status['scroll']['y'])/128))%3]
       point=(round((origin[0]+x)*scale),round((origin[1]+y)*scale));actual=capture.getpixel(point)
       if max(abs(actual[i]-expected[i]) for i in range(3))>3:return False
       samples.append(dict(pixel=point,rgb=actual))
      return samples
    paintedSamples=wait(painted_edges)
    with Image.open(image) as source:
     capture=source.convert('RGB');pixels=capture.load();sentinels=[(255,0,255),(0,255,0),(0,255,255)]
     def vivid(x,y):return any(max(abs(pixels[x,y][i]-color[i]) for i in range(3))<=3 for color in sentinels)
     assert abs(capture.width-display['current_mode']['width'])<=1 and abs(capture.height-display['current_mode']['height'])<=1
     surface=status['surface'];origin=(box['x']+client['x']+surface['x']-screen['x'],box['y']+client['y']+surface['y']-screen['y'])
     radius=status['maskRadius'] if status['maskActive'] else 0
     violations=[];painted=0
     left=raster_origin(origin[0],scale);right=raster_origin(origin[0]+surface['width'],scale)
     top=raster_origin(origin[1],scale);bottomPixel=raster_origin(origin[1]+surface['height'],scale)
     band=math.ceil(max(radius,frame_radius)*scale)+4
     coordinates=set()
     for py in [*range(top-4,top+band),*range(bottomPixel-band,bottomPixel+4)]:
      coordinates.update((px,py) for px in range(left-4,right+4))
     for px in [*range(left-4,left+band),*range(right-band,right+4)]:
      coordinates.update((px,py) for py in range(top-4,bottomPixel+4))
     for px,py in coordinates:
      if not 0<=px<capture.width or not 0<=py<capture.height:continue
      x=(px+.5-left)/scale;y=(py+.5-top)/scale
      if not vivid(px,py):continue
      painted+=1
      outside=outside_frame(x,y,surface['width'],surface['height'],radius,1/scale)
      if a.effects:
       outer={'x':box['x']-screen['x'],'y':box['y']-current['deco_rect']['height']-screen['y'],'width':box['width'],'height':box['height']+current['deco_rect']['height']}
       fx=(px+.5-raster_origin(outer['x'],scale))/scale;fy=(py+.5-raster_origin(outer['y'],scale))/scale;frameRadius=frame_radius
       outside=outside or outside_frame(fx,fy,outer['width'],outer['height'],frameRadius,1/scale)
      if outside:violations.append([px,py])
     record=dict(scale=scale,size=[width,height],bottom=bottom,status=status,physicalOrigin=[left,top],compositorRadius=frame_radius,window=current,output=display,capture=str(image),painted=painted,paintedSamples=paintedSamples,violations=violations[:30])
     (out/'last-state.json').write_text(json.dumps(record,indent=2))
     assert painted>100*scale*scale,record
     assert not violations,record
     records.append(record)
    # Exercise actual unclipped production search and the boundary viewport.
    target=status['search'];assert target,status
    click(box['x']+client['x']+target['x']+target['width']/2,box['y']+client['y']+target['y']+target['height']/2)
    wait(lambda:state()['searchFocused'])
    call('wtype','-M','ctrl','-k','a','-m','ctrl','edge');wait(lambda:state()['query']=='edge')
    call('wtype','-M','ctrl','-k','a','-m','ctrl','-k','BackSpace');wait(lambda:state()['query']=='')
    if not bottom:
     # The sentinel is programmatically scrolled to exact clipping positions;
     # real pointer input exercises the unchanged production scroll viewport.
     call('quickshell','-p',str(qml),'ipc','call','clipping','resetViewport')
     viewport=state()['viewport'];scrollNeeded=viewport['contentHeight']>viewport['rect']['height']
     targetX=viewport['rect']['x']+10;targetY=viewport['rect']['y']+viewport['rect']['height']/2
     ipc(f'seat seat0 cursor set {round(origin[0]+targetX)} {round(origin[1]+targetY)}')
     pointer.stdin.write('m 0 0\n');pointer.stdin.flush()
     wait(lambda:state()['viewport']['hovered'] and abs(state()['viewport']['pointerX']-10)<=1 and abs(state()['viewport']['pointerY']-viewport['rect']['height']/2)<=1)
     pointer.stdin.write('w 1\n');pointer.stdin.flush()
     if scrollNeeded:wait(lambda:state()['viewport']['y']>0)
     else:assert state()['viewport']['y']==0,state()
     record['productionScrollInput']=dict(overflow=scrollNeeded,before=viewport,after=state()['viewport'])
 # A deliberately oversized client must be fitted using an outer budget,
 # then repeated requests must preserve its new size and explicit placement.
 before=window(title);padWidth=before['rect']['width']-before['window_rect']['width'];padHeight=before['rect']['height']+before['deco_rect']['height']-before['window_rect']['height']
 ipc(f'[con_id={identity}] resize set width {1200+padWidth} px height {900+padHeight} px')
 wait(lambda:state()['width']+padWidth>outputs()['HEADLESS-1']['rect']['width']-32 and state()['height']+padHeight>outputs()['HEADLESS-1']['rect']['height']-150)
 completed=state()['fit']['completed']
 call('quickshell','-p',str(qml),'ipc','call','clipping','fitRepeated')
 wait(lambda:state()['fit']['completed']>completed and not state()['fit']['busy'])
 fitted=state();assert not fitted['fit']['error'],fitted
 outcome=fitted['fit']['result'];assert outcome['supported'] and outcome['changed'],outcome
 wait(lambda:state()['width']==outcome['clientWidth'] and state()['height']==outcome['clientHeight']);fitted=state()
 assert outcome['outerWidth']<=outcome['budgetWidth'] and outcome['outerHeight']<=outcome['budgetHeight'],outcome
 afterFit=window(title);completed=fitted['fit']['completed']
 call('quickshell','-p',str(qml),'ipc','call','clipping','fitRepeated')
 wait(lambda:state()['fit']['completed']>completed and not state()['fit']['busy'])
 stable=state();assert not stable['fit']['error'] and not stable['fit']['result']['changed'],stable
 assert stable['fit']['completed']==completed+1,stable
 assert window(title)['rect']==afterFit['rect'],(afterFit,window(title))
 fitProof=dict(first=fitted,repeat=stable,window=afterFit)
 # Enter an already-existing smaller display: its size signals do not change.
 # The screen-identity change itself must schedule a fresh exact-target fit.
 beforeMove=state();completed=beforeMove['fit']['completed']
 ipc(f'[con_id={identity}] move container to output HEADLESS-2')
 wait(lambda:state()['screen']=='HEADLESS-2' and state()['fit']['completed']>completed and not state()['fit']['busy'])
 small=state();assert not small['fit']['error'],small
 smallOutcome=small['fit']['result'];assert smallOutcome['changed'],small
 wait(lambda:state()['width']==smallOutcome['clientWidth'] and state()['height']==smallOutcome['clientHeight'])
 assert smallOutcome['outerWidth']<=outputs()['HEADLESS-2']['rect']['width']-32 and smallOutcome['outerHeight']<=outputs()['HEADLESS-2']['rect']['height']-150,small
 small=state();smallWindow=window(title);assert smallWindow['id']==identity
 call('grim','-o','HEADLESS-2','-s','1.25',str(out/'fit-small-output.png'))
 completed=small['fit']['completed']
 ipc(f'[con_id={identity}] move container to output HEADLESS-1')
 wait(lambda:state()['screen']=='HEADLESS-1' and state()['fit']['completed']>completed and not state()['fit']['busy'])
 returned=state();assert not returned['fit']['error'],returned
 assert (returned['width'],returned['height'])==(small['width'],small['height']),returned
 fitProof.update(smallerOutput=dict(state=small,window=smallWindow),returnOutput=returned)
 ipc('focus output HEADLESS-1')
 # An identically titled window in the same process is ambiguous, never a
 # reason to resize the first matching container. Then reject its retired ID.
 call('quickshell','-p',str(qml),'ipc','call','clipping','auxiliary',title,'0')
 def matching():return [node for node in nodes(json.loads(call('swaymsg','-t','get_tree','-r'))) if node.get('app_id')=='org.quickshell' and node.get('name')==title]
 wait(lambda:len(matching())==2 and all(node['window_rect']['width']==state()['auxiliary']['width'] and node['window_rect']['height']==state()['auxiliary']['height'] for node in matching() if node['id']!=identity))
 duplicateId=next(node['id'] for node in matching() if node['id']!=identity)
 beforeAmbiguous={node['id']:node['rect'] for node in matching()};completed=state()['fit']['completed']
 call('quickshell','-p',str(qml),'ipc','call','clipping','fitRepeated')
 wait(lambda:state()['fit']['completed']>completed and not state()['fit']['busy'])
 assert state()['fit']['error'] and {node['id']:node['rect'] for node in matching()}==beforeAmbiguous,state()
 call('quickshell','-p',str(qml),'ipc','call','clipping','hideAuxiliary');wait(lambda:len(matching())==1)
 literalTitle=r'Literal "quote"; [x] \ café — Ω'+'\nline'
 call('quickshell','-p',str(qml),'ipc','call','clipping','auxiliary',literalTitle,str(duplicateId))
 literal=wait(lambda:window(literalTitle) if window(literalTitle) and window(literalTitle)['window_rect']['width']==state()['auxiliary']['width'] and window(literalTitle)['window_rect']['height']==state()['auxiliary']['height'] else None);assert literal['id']!=duplicateId
 beforeLiteral=literal['rect'];completed=state()['auxiliary']['completed']
 call('quickshell','-p',str(qml),'ipc','call','clipping','fitAuxiliary')
 wait(lambda:state()['auxiliary']['completed']>completed and not state()['auxiliary']['busy'])
 assert state()['auxiliary']['error'] and window(literalTitle)['rect']==beforeLiteral,state()
 call('quickshell','-p',str(qml),'ipc','call','clipping','auxiliary',literalTitle,'0')
 completed=state()['auxiliary']['completed'];call('quickshell','-p',str(qml),'ipc','call','clipping','fitAuxiliary')
 wait(lambda:state()['auxiliary']['completed']>completed and not state()['auxiliary']['busy'])
 literalProof=state()['auxiliary'];assert not literalProof['error'] and literalProof['result']['changed'],literalProof
 wait(lambda:state()['auxiliary']['width']==literalProof['result']['clientWidth'] and state()['auxiliary']['height']==literalProof['result']['clientHeight'])
 assert literalProof['result']['pid']==window(literalTitle)['pid']
 call('quickshell','-p',str(qml),'ipc','call','clipping','hideAuxiliary');wait(lambda:window(literalTitle) is None)
 fitProof.update(ambiguousRejected=True,retiredContainerRejected=True,literalTitle=literalProof)
 current=window(title);status=state();box=current['rect'];client=current['window_rect']
 if a.native_frame=='on':
  # Shared native control centres are 14 + 6 and titlebar midline in the patch.
  closePoint=[box['x']+20,box['y']-current['deco_rect']['height']/2]
 else:
  button=status['closeControl'];assert button,status
  closePoint=[box['x']+client['x']+button['x']+button['width']/2,box['y']+client['y']+button['y']+button['height']/2]
 click(*closePoint);wait(lambda:window(title) is None)
 (out/'result.json').write_text(json.dumps(dict(passed=True,cases=records,source=call('git','-C',str(repo),'rev-parse','HEAD').strip(),fixtureSha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),closePoint=closePoint,closePassed=True,fitProof=fitProof,launcher=str(a.launcher),compositor=a.sway,pointer=a.pointer_tool,backend=a.content_backend,nativeFrame=a.native_frame,effects=a.effects,assetsSha256={str(path.relative_to(assets)):hashlib.sha256(path.read_bytes()).hexdigest() for path in sorted(assets.rglob('*')) if path.is_file()}),indent=2))
finally:
 (out/'process-status.json').write_text(json.dumps([dict(argv=process.args,returncode=process.poll()) for process in processes],indent=2))
 for process in reversed(processes):
  if process.poll() is None:
   os.killpg(process.pid,signal.SIGTERM)
   try:process.wait(timeout=5)
   except sp.TimeoutExpired:os.killpg(process.pid,signal.SIGKILL);process.wait(timeout=5)
 close_reaped_pipes(processes)
 private.cleanup()

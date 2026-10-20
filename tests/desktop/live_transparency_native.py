"""Live material policy on mapped native windows, popovers and a real Dock."""
import argparse,hashlib,json,os,re,shutil,signal,subprocess as sp,sys,tempfile,time
from pathlib import Path
from private_native_bus import start_private_bus
from fixture_resources import close_reaped_pipes
p=argparse.ArgumentParser();p.add_argument('--launcher',type=Path,required=True);p.add_argument('--sway',required=True);p.add_argument('--pointer-tool',required=True);p.add_argument('--output',type=Path,required=True);p.add_argument('--effects',action='store_true');p.add_argument('--surfaces',nargs='+',choices=['remote','help','session','panel','calendar','notification','dock'],default=['remote','help','session','panel','calendar','notification','dock']);a=p.parse_args();a.output.mkdir(parents=True,exist_ok=False)
repo=Path(__file__).resolve().parents[2];processes=[];original_socket=os.environ.get('SWAYSOCK')
with tempfile.TemporaryDirectory(prefix='lumen-materials-') as temporary:
 private=Path(temporary);prefix=re.split(r'\n[ \t]*exec quickshell ',a.launcher.read_text(),maxsplit=1)[0]
 env=dict(v.decode().split('=',1) for v in sp.check_output(['bash','-c',prefix+'\nexec env -0']).split(b'\0') if b'=' in v)
 for name in ('HOME','XDG_RUNTIME_DIR','XDG_CONFIG_HOME','XDG_STATE_HOME','XDG_DATA_HOME','XDG_CACHE_HOME'):env[name]=str(private/name);Path(env[name]).mkdir(mode=0o700)
 for name in ('DISPLAY','WAYLAND_DISPLAY','SWAYSOCK','DBUS_SESSION_BUS_ADDRESS','AT_SPI_BUS_ADDRESS','PULSE_SERVER','PIPEWIRE_REMOTE'):env.pop(name,None)
 env.update(WLR_BACKENDS='headless',WLR_RENDERER='gles2' if a.effects else 'pixman',WLR_RENDERER_FORCE_SOFTWARE='1' if a.effects else '0',WLR_RENDERER_ALLOW_SOFTWARE='1',LIBGL_ALWAYS_SOFTWARE='1',WLR_LIBINPUT_NO_DEVICES='1',QT_QPA_PLATFORM='wayland',QT_QUICK_BACKEND='software',QT_QPA_PLATFORMTHEME='generic',QT_NO_XDG_DESKTOP_PORTAL='1',QT_ACCESSIBILITY='0',NO_AT_BRIDGE='1',GTK_A11Y='none',LUMEN_COMPOSITOR_EFFECTS='1' if a.effects else '0',LUMEN_UNIFIED_OWNED_HEADERS='1',LUMEN_NATIVE_WINDOW_CONTROLS='0')
 assets=Path(re.search(r'export LUMEN_NOTIFICATION_HELPER=(\S+)',a.launcher.read_text()).group(1)).parent.parent
 shutil.copytree(assets,private/'assets');palette=json.loads((assets/'appearance/palette.json').read_text());appearance=private/'appearance.json';env['LUMEN_APPEARANCE_STATE']=str(appearance)
 def theme(mode,reduced):
  value=dict(font=palette['font'],palette=palette[mode],preferences=dict(mode=mode,reduceMotion=True,reduceTransparency=reduced,dockMagnification=False,dockSize=48,dockAutoHide=False))
  pending=appearance.with_suffix('.pending');pending.write_text(json.dumps(value));pending.replace(appearance)
 theme('dark',False)
 qml=private/'fixture.qml';qml.write_text('''import QtQuick
import QtQuick.Controls
import QtQuick.Window
import Quickshell
import Quickshell.Wayland
import Quickshell.Io
import "assets/shell" as D
ShellRoot {
 id:root
 property string active:""
 property var records:[]
 function record(value){records=records.concat(value);}
 QtObject {id:stub
  property string popupOwner:""
  property date currentDate:new Date(2026,9,20)
  property bool dockPreferencesBusy:false
  property string aliasError:""
  property string applicationCommandError:""
  property var referenceDock:[{key:"one",action:"fixture",image:"files.svg",canvas:68},{key:"two",action:"fixture",image:"files.svg",canvas:68},{key:"three",action:"fixture",image:"calendar.svg",canvas:68}]
  signal requestDockFocus(string name)
  signal dismissPopups(string owner)
  function claimPopup(owner){popupOwner=owner;dismissPopups(owner);}
  function preferredOutput(){return "HEADLESS-1";}
  function dockWindows(item){return [];}
  function dockLabel(item){return item.key;}
  function runningFor(entry){return false;}
  function activateDock(item,shift,name){root.record("dock:"+item.key);}
  function canConfigureDockEntry(entry){return false;}
  function isDockPinned(entry){return false;}
 }
 QtObject {id:notifications
  property bool available:true
  property var policies:({})
  property var expandedCards:({})
  function invoke(row,key){root.record("notification:"+row.id+":"+key);}
  function command(value){}
 }
 PanelWindow {id:backdrop;anchors {left:true;right:true;top:true;bottom:true}exclusionMode:ExclusionMode.Ignore;exclusiveZone:0;color:"transparent";WlrLayershell.layer:WlrLayer.Background;WlrLayershell.namespace:"lumen-material-test-background"
  Repeater {model:Math.ceil(backdrop.width/32)*Math.ceil(backdrop.height/32)
   Rectangle {required property int index;x:(index%Math.ceil(backdrop.width/32))*32;y:Math.floor(index/Math.ceil(backdrop.width/32))*32;width:32;height:32;color:((Math.floor(x/32)+Math.floor(y/32))%2)?"#e020a0":"#10d0e0"}
  }
 }
 D.Dock {id:dock;shell:stub;screen:Quickshell.screens[0]}
 D.RemoteDesktopWindow {id:remote;shell:stub}
 D.HelpWindow {id:help;shell:stub}
 D.SessionOptions {id:session;executeActions:false;onConfirmed:action=>root.record("session:"+action)}
 PanelWindow {id:popover;visible:root.active==="panel";anchors {top:true;right:true}margins {top:80;right:50}implicitWidth:320;implicitHeight:140;exclusionMode:ExclusionMode.Ignore;color:"transparent";WlrLayershell.layer:WlrLayer.Overlay;WlrLayershell.keyboardFocus:WlrKeyboardFocus.OnDemand
  D.PanelSurface {id:panel;anchors.fill:parent
   Column {x:18;y:24;width:parent.width-36;spacing:12
    D.PanelLabel {text:"Material policy";font.weight:Font.DemiBold}
    D.ControlRow {id:panelAction;width:parent.width;text:"Run private panel action";onClicked:root.record("panel")}
   }
  }
 }
 PanelWindow {id:calendarPopup;visible:root.active==="calendar";anchors {top:true;right:true}margins {top:80;right:50}implicitWidth:344;implicitHeight:calendar.height;exclusionMode:ExclusionMode.Ignore;color:"transparent";WlrLayershell.layer:WlrLayer.Overlay;WlrLayershell.keyboardFocus:WlrKeyboardFocus.OnDemand
  D.CalendarCard {id:calendar;width:parent.width;today:new Date(2026,9,20);compact:true;active:false;onOpenCalendar:day=>root.record("calendar:"+Qt.formatDateTime(day,"yyyy-MM-dd"))}
 }
 PanelWindow {id:banner;visible:root.active==="notification";anchors {top:true;right:true}margins {top:80;right:50}implicitWidth:350;implicitHeight:card.height;exclusionMode:ExclusionMode.Ignore;color:"transparent";WlrLayershell.layer:WlrLayer.Overlay;WlrLayershell.keyboardFocus:WlrKeyboardFocus.OnDemand
  D.NotificationCard {id:card;width:parent.width;service:notifications;notification:({id:7,app:"Private fixture",desktop:"",summary:"Stable notification title",body:"An owned notification remains readable.",icon:"",image:"",timestamp:Date.now()+3600000,actions:[{id:"default",label:"Open"}],urgency:1,resident:true,transient:false,banner:true,unread:true})}
 }
 function find(item,name){if(item.objectName===name || (item.Accessible && item.Accessible.name===name))return item;for(const child of item.children || []){const found=find(child,name);if(found)return found;}if(item.contentItem && item.contentItem!==item){const found=find(item.contentItem,name);if(found)return found;}return null;}
 function owner(){return ({remote:remote,help:help,session:session,panel:popover,calendar:calendarPopup,notification:banner,dock:dock})[active];}
 function selectedTarget(){const window=owner();return active==="remote"?find(remote.contentItem,"VNC"):active==="help"?find(help.contentItem,"Search Help"):active==="session"?find(session.contentItem,"Cancel"):active==="panel"?panelAction:active==="calendar"?find(calendar,"calendar-open-related"):active==="notification"?card:null;}
 function rect(item,reference){const at=item.mapToItem(reference || root.owner().contentItem,0,0);return {x:at.x,y:at.y,width:item.width,height:item.height};}
 function dockMaterial(item){if(!item.visible)return null;if(item.radius===22 && item.width===dock.width && item.height===dock.shelfThickness && item.color && item.color.a>0)return item.color;for(const child of item.children || []){const color=dockMaterial(child);if(color)return color;}return null;}
 function material(){if(active==="panel")return panel.color;if(active==="calendar")return calendar.color;if(active==="notification")return card.color;if(active==="remote")return D.Theme.sidebarMaterial;if(active==="dock")return dockMaterial(dock.contentItem) || Qt.color(D.Theme.colors.background);return Qt.color(D.Theme.colors.background);}
 IpcHandler {target:"transparencyAudit"
  function open(name:string):void {remote.visible=false;help.visible=false;session.visible=false;root.active=name;root.records=[];if(name==="remote"){remote.filter="All";remote.visible=true;}else if(name==="help"){find(help.contentItem,"Search Help").text="";help.show();}else if(name==="session")session.show("suspend");}
  function focusDock():void{stub.requestDockFocus("HEADLESS-1");}
  function focusTarget():bool {const item=root.selectedTarget();if(!item)return false;item.forceActiveFocus();return item.activeFocus;}
  function state():string {
   const window=root.owner(),item=root.selectedTarget();let actions=[],texts=[];
   function scan(item){if(!item.visible || item.width<=0 || item.height<=0)return;
    if(item.Accessible && item.Accessible.name && item.enabled && (typeof item.clicked==="function" || item.activeFocusOnTab))actions.push({name:item.Accessible.name,rect:rect(item)});
    if(item.text!==undefined && item.font!==undefined && String(item.text))texts.push({text:String(item.text),rect:rect(item),size:item.font.pixelSize,truncated:!!item.truncated});
    for(const child of item.children || [])scan(child);
   }
   if(window)scan(window.contentItem);
   const color=root.material();
   return JSON.stringify({active:active,helpQuery:help.query,reduced:!!D.Theme.preferences.reduceTransparency,dark:D.Theme.dark,window:window?rect(window.contentItem):null,layerMargins:window && window.margins!==undefined?{top:window.margins.top,right:window.margins.right}:null,target:item?rect(item):null,focused:item?item.activeFocus:false,actions:actions,texts:texts,records:root.records,filter:remote.filter,visible:window?window.visible:false,material:{r:color.r,g:color.g,b:color.b,a:color.a},dock:{rect:rect(dock.contentItem,dock.contentItem),width:dock.width,height:dock.height,marginBottom:dock.margins.bottom,slot:dock.slot,hovered:dock.hovered,keyboard:dock.keyboardMode,keyboardIndex:dock.keyboardIndex,mask:{x:dock.mask.x,y:dock.mask.y,width:dock.mask.width,height:dock.mask.height},zone:dock.exclusiveZone,keys:stub.referenceDock.map(item=>item.key)}});
  }
 }
}''')
 def launch(args,name,**kw):
  with (a.output/name).open('w') as stream:proc=sp.Popen(args,env=env,stdout=stream,stderr=sp.STDOUT,start_new_session=True,**kw)
  processes.append(proc);return proc
 def call(*args):return sp.check_output(args,env=env,text=True,timeout=6).strip()
 def sway(*args):return json.loads(call(str(Path(a.sway).parent/'swaymsg'),'-r',*args))
 def ipc(method,*args):return call('quickshell','-p',str(qml),'ipc','call','transparencyAudit',method,*args)
 def state():
  value=json.loads(ipc('state'))
  name=value['active'];titles=dict(remote='Remote Desktop',help='Desktop Help',session='Session Options')
  if name in titles:
   observed=native(titles[name]);origin=(observed['rect']['x'],observed['rect']['y']) if observed else (0,0)
   if observed is None:value['window']=None
  elif name=='dock':origin=(output_rect['x']+(output_rect['width']-value['dock']['width'])/2,output_rect['y']+output_rect['height']-value['dock']['height']-value['dock']['marginBottom'])
  else:origin=(output_rect['x']+output_rect['width']-(value['window'] or {}).get('width',0)-(value['layerMargins'] or {}).get('right',0),output_rect['y']+(value['layerMargins'] or {}).get('top',0))
  def offset(rect,origin):
   if rect:rect['x']+=origin[0];rect['y']+=origin[1]
  for rect in [value['window'],value['target']]+[row['rect'] for row in value['actions']+value['texts']]:offset(rect,origin)
  offset(value['dock']['rect'],(output_rect['x']+(output_rect['width']-value['dock']['width'])/2,output_rect['y']+output_rect['height']-value['dock']['height']-value['dock']['marginBottom']))
  (a.output/'last-state.json').write_text(json.dumps(value,indent=2));return value
 def wait(fn):
  deadline=time.monotonic()+12
  while time.monotonic()<deadline:
   try:
    value=fn()
    if value:return value
   except (OSError,ValueError,sp.SubprocessError):pass
   time.sleep(.04)
  raise AssertionError('Live material acceptance timed out: '+str(a.output))
 def nodes(n):
  yield n
  for child in n.get('nodes',[])+n.get('floating_nodes',[]):yield from nodes(child)
 def native(title):return next((n for n in nodes(sway('-t','get_tree')) if n.get('app_id')=='org.quickshell' and n.get('name')==title),None)
 def pointer_event(text):pointer.stdin.write(text+'\n');pointer.stdin.flush();time.sleep(.05)
 def move(x,y):
  sway('seat seat0 cursor set %d %d'%(x,y));pointer_event('m 0.1 0.1')
 def geometry(value):return {key:value[key] for key in ('window','target','actions','texts','dock') if key!='dock'}
 try:
  start_private_bus(env,a.output,processes)
  config=private/'sway.conf';config.write_text('output HEADLESS-1 resolution 1600x1000\nseat seat0 fallback true\nfocus_follows_mouse no\nxwayland disable\nfor_window [all] floating enable, border pixel 0, move position 40 60\n')
  launch([a.sway,'--unsupported-gpu','-c',str(config)],'sway.log');env['SWAYSOCK']=str(wait(lambda:next(Path(env['XDG_RUNTIME_DIR']).glob('sway-ipc*.sock'),None)));env['WAYLAND_DISPLAY']=next(p.name for p in Path(env['XDG_RUNTIME_DIR']).glob('wayland-*') if not p.name.endswith('.lock'))
  output_rect=next(row['rect'] for row in sway('-t','get_outputs') if row.get('active') and row['name']=='HEADLESS-1')
  launch(['wtype','-s','120000'],'keyboard.log');pointer=launch([a.pointer_tool,'pointer','move','0','0'],'pointer.log',stdin=sp.PIPE,text=True);launch(['quickshell','-p',str(qml)],'shell.log');wait(lambda:ipc('state'))
  from PIL import Image,ImageStat
  results=[]
  titles=dict(remote='Remote Desktop',help='Desktop Help',session='Session Options')
  for mode in ('light','dark'):
   for name in a.surfaces:
    theme(mode,False);wait(lambda:state()['dark']==(mode=='dark') and not state()['reduced']);ipc('open',name);before=wait(lambda:state() if state()['window'] and state()['window']['width']>0 and (name=='dock' or state()['target']) else None)
    native_before=wait(lambda:native(titles[name])) if name in titles else None
    identifier=native_before['id'] if native_before else None
    if name=='dock':
     d=before['dock'];point=(d['rect']['x']+12+1.5*d['slot'],d['rect']['y']+d['height']-35)
    else:
     r=before['target'];point=(r['x']+r['width']/2,r['y']+r['height']-8 if name=='notification' else r['y']+r['height']/2)
    move(*point);pointer_event('p');before=state();frames=[]
    for index,reduced in enumerate((False,True,False)):
     theme(mode,reduced);wait(lambda:state()['reduced']==reduced);current=state();assert geometry(current)==geometry(before),(name,mode,reduced,'interaction geometry changed',before,current)
     assert current['focused']==before['focused'],(name,'keyboard focus changed')
     if identifier:assert native(titles[name])['id']==identifier and native(titles[name])['rect']==native_before['rect']
     if name=='dock':assert current['dock']==before['dock'],('Dock geometry/mask changed',current,before)
     path=a.output/f'{name}-{mode}-{index}.png'
     def painted():
      call('grim',str(path));window=current['window'];box=tuple(round(value) for value in (window['x'],window['y'],window['x']+window['width'],window['y']+window['height']))
      with Image.open(path) as image:
       image=image.convert('RGB');crop=image.crop(box);crop.save(path.with_name(path.stem+'-content.png'))
       color=current['material'];alpha=color['a'];rgb=[color[channel]*255 for channel in ('r','g','b')]
       points=[]
       for dx in range(-32,33,4):
        x=round(window['x']+(75 if name=='remote' else window['width']/2)+dx);y=round(window['y']+(window['height']-4 if name=='dock' else 8))
        under=tuple(int(palette[mode]['background'][i:i+2],16) for i in (1,3,5)) if name=='remote' else ((224,32,160) if (x//32+y//32)%2 else (16,208,224))
        expected=tuple(round(rgb[i]*alpha+under[i]*(1-alpha)) for i in range(3));actual=image.getpixel((x,y));points.append(dict(x=x,y=y,actual=actual,expected=expected,error=max(abs(a-b) for a,b in zip(actual,expected))))
       matches=sum(point['error']<=3 for point in points)
       foreground=tuple(int(palette[mode]['text'][i:i+2],16) for i in (1,3,5))
       text_pixels=sum(max(abs(c[i]-foreground[i]) for i in range(3))<=3 for c in crop.get_flattened_data()) if name!='dock' else 0
       def linear(value):
        value=value/255;return value/12.92 if value<=.04045 else ((value+.055)/1.055)**2.4
       def lightness(rgb):return sum(linear(c)*weight for c,weight in zip(rgb,(.2126,.7152,.0722)))
       ink=lightness(foreground);contrasts=[(max(ink,lightness(p['actual']))+.05)/(min(ink,lightness(p['actual']))+.05) for p in points]
       diagnostics=dict(points=points,matches=matches,textPixels=text_pixels,material=color,minimumTextContrast=min(contrasts))
       (a.output/'last-paint.json').write_text(json.dumps(diagnostics,indent=2))
       if matches<len(points)*.9 or (name!='dock' and text_pixels<10):return None
       if name!='dock':assert min(contrasts)>=4.5,(name,'foreground contrast',min(contrasts))
       assert max(ImageStat.Stat(crop).stddev)>2,(name,'unpainted interior')
       return diagnostics
     paint=wait(painted)
     frames.append(dict(reduced=reduced,state=current,capture=str(path),paint=paint))
     (a.output/'last-frames.json').write_text(json.dumps(frames,indent=2))
     if reduced:assert current['material']['a']==1,(name,'global fallback remains translucent',current)
    pointer_event('r');after=state()
    if name=='remote':assert after['filter']=='VNC'
    elif name=='help':assert after['focused']
    elif name=='session':assert not after['visible'] and not after['records']
    else:assert after['records']==[dict(panel='panel',calendar='calendar:2026-10-20',notification='notification:7:default',dock='dock:two')[name]],(name,after)
    if name=='help':
     call('wtype','k');wait(lambda:state()['helpQuery']=='k')
    elif name in ('panel','calendar','notification'):
     if name=='notification':assert ipc('focusTarget')=='true'
     call('wtype','-k','space');wait(lambda:len(state()['records'])==2);assert state()['records'][0]==state()['records'][1]
    elif name=='dock':
     ipc('focusDock');wait(lambda:state()['dock']['keyboard']);call('wtype','-k','Home','-k','Right','-k','Return');wait(lambda:len(state()['records'])==2);assert state()['records']==['dock:two','dock:two']
    results.append(dict(surface=name,mode=mode,nativeId=identifier,frames=frames,after=after,afterKeyboard=state()))
  log=(a.output/'shell.log').read_text();assert not any(x in log for x in ('ReferenceError','TypeError','Binding loop','Failed to load','PyGIDeprecationWarning','WARN scene:')),log
  (a.output/'result.json').write_text(json.dumps(dict(passed=True,effects=a.effects,checks=results,sourceHashes={str(p.relative_to(assets)):hashlib.sha256(p.read_bytes()).hexdigest() for p in [assets/'shell/CalendarCard.qml',assets/'shell/NotificationCard.qml',assets/'shell/Theme.qml',assets/'shell/Dock.qml']},liveSocketUnchanged=os.environ.get('SWAYSOCK')==original_socket),indent=2));print('PASS live transparency',len(results))
 finally:
  if 'pointer' in locals() and pointer.stdin and not pointer.stdin.closed:pointer.stdin.close()
  for proc in reversed(processes):
   if proc.poll() is None:os.killpg(proc.pid,signal.SIGTERM)
  for proc in reversed(processes):
   try:proc.wait(timeout=4)
   except sp.TimeoutExpired:os.killpg(proc.pid,signal.SIGKILL);proc.wait(timeout=4)
   for stream in (proc.stdin,proc.stdout,proc.stderr):
    if stream and not stream.closed:stream.close()
  close_reaped_pipes(processes)

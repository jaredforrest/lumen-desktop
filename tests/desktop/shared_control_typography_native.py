"""Physical shared control and constrained calendar-editor keyboard acceptance."""
import argparse,hashlib,json,os,re,shutil,signal,subprocess as sp,sys,time
from pathlib import Path
from application_fixture import isolation
from application_fixture.keyboard import PhysicalKeyboard
from private_native_bus import start_private_bus
from fixture_resources import close_reaped_pipes
p=argparse.ArgumentParser();p.add_argument('--launcher',type=Path,required=True);p.add_argument('--sway',type=Path,required=True);p.add_argument('--keyboard',type=Path,required=True);p.add_argument('--pointer-tool',type=Path,required=True);p.add_argument('--font-config',type=Path,required=True);p.add_argument('--software-driver',type=Path,required=True);p.add_argument('--effects',action='store_true');p.add_argument('--protocol-trace',action='store_true',required=True);p.add_argument('--inside',action='store_true');p.add_argument('--output',type=Path,required=True);a=p.parse_args();repo=Path(__file__).resolve().parents[2]
for name in ('launcher','sway','keyboard','pointer_tool','font_config','software_driver'):
 value=getattr(a,name).resolve();assert str(value).startswith('/nix/store/')and value.exists(),name;setattr(a,name,value)
if not a.inside:
 assert not sp.check_output(['git','-C',str(repo),'status','--porcelain'],text=True).strip(),'Freeze source before acceptance'
 revision=sp.check_output(['git','-C',str(repo),'rev-parse','HEAD'],text=True).strip();out=a.output.resolve();out.mkdir(mode=0o700,parents=True,exist_ok=False);prefix=re.split(r'\n[ \t]*exec quickshell ',a.launcher.read_text(),maxsplit=1)[0];env=dict(v.decode().split('=',1)for v in sp.check_output(['bash','-c',prefix+'\nexec env -0']).split(b'\0')if b'='in v);env['FONTCONFIG_FILE']=str(a.font_config)
 command=[sys.executable,'/source/tests/desktop/shared_control_typography_native.py','--inside','--output','/artifacts']+sum(([ '--'+name.replace('_','-'),str(getattr(a,name))]for name in ('launcher','sway','keyboard','pointer_tool','font_config','software_driver')),[])+(['--effects']if a.effects else [])+(['--protocol-trace']if a.protocol_trace else []);source_hash=hashlib.sha256(Path(__file__).read_bytes()).hexdigest();code=isolation.run(repo,out,command,env,software_graphics=a.effects);unchanged=sp.check_output(['git','-C',str(repo),'rev-parse','HEAD'],text=True).strip()==revision and not sp.check_output(['git','-C',str(repo),'status','--porcelain'],text=True).strip();(out/'provenance.json').write_text(json.dumps(dict(revision=revision,sourceSha256=source_hash,sourceUnchanged=unchanged,exitCode=code,protocolTrace=a.protocol_trace,launcher=str(a.launcher),compositor=str(a.sway),fontConfig=str(a.font_config)),indent=2));assert unchanged;raise SystemExit(code)
assert repo==Path('/source')and os.environ['HOME']=='/home/fixture'and os.getuid()!=0 and not Path('/.old-root').exists();assert not any((Path('/dev')/name).exists()for name in ('dri','snd','video0','tty'))
out=a.output;env=dict(os.environ,FONTCONFIG_FILE=str(a.font_config),LANG='C.UTF-8',LC_ALL='C.UTF-8',WLR_BACKENDS='headless',WLR_HEADLESS_OUTPUTS='1',WLR_RENDERER='gles2'if a.effects else 'pixman',WLR_RENDERER_FORCE_SOFTWARE='1'if a.effects else '0',WLR_RENDERER_ALLOW_SOFTWARE='1',WLR_LIBINPUT_NO_DEVICES='1',LIBGL_ALWAYS_SOFTWARE='1',MESA_LOADER_DRIVER_OVERRIDE='llvmpipe',LIBGL_DRIVERS_PATH=str(a.software_driver/'lib/dri'),__EGL_VENDOR_LIBRARY_FILENAMES=str(a.software_driver/'share/glvnd/egl_vendor.d/50_mesa.json'),QT_QPA_PLATFORM='wayland',QT_QUICK_BACKEND='software',QT_QPA_PLATFORMTHEME='generic',QT_ACCESSIBILITY='0',NO_AT_BRIDGE='1',GTK_A11Y='none',LUMEN_UNIFIED_OWNED_HEADERS='1',LUMEN_NATIVE_WINDOW_CONTROLS='0',LUMEN_NATIVE_WINDOW_ACTIONS='0',LUMEN_FILE_SERVICE='0',LUMEN_LAUNCH_SCOPES='0',LUMEN_NEARBY_SHARING_PROVIDER='',GLOBAL_MENU_PYTHON=sys.executable)
for key in ('DISPLAY','WAYLAND_DISPLAY','SWAYSOCK'):env.pop(key,None)
assets=out/'desktop';shutil.copytree(repo/'res/lumen-desktop/shell',assets);palette=json.loads((repo/'res/lumen-desktop/appearance/palette.json').read_text());appearance=out/'appearance.json';env['LUMEN_APPEARANCE_STATE']=str(appearance);appearance.write_text(json.dumps(dict(font=palette['font'],palette=palette['light'],preferences=dict(textScale=1.5,reduceMotion=True,reduceTransparency=True))))
binpath=out/'bin';binpath.mkdir();env['PATH']=str(binpath)+':'+env['PATH'];actions=out/'provider-actions.jsonl';payload=dict(events=[],days=[],total=0,configured=True,providers=[dict(id='google',name='Private provider',generation='private-generation',configured=True,calendars=[dict(id='private',name='Private calendar',accessRole='owner')])])
calendar=binpath/'lumen-calendar';calendar.write_text('#!'+sys.executable+'\nimport json,sys\nr=json.loads(sys.stdin.readline())\nwith open('+repr(str(actions))+',"a")as f:f.write(json.dumps(dict(provider="calendar",request=r))+"\\n")\nprint(json.dumps(dict(error="Private provider rejects mutations")) if r.get("action")in("create","update","delete") else '+repr(json.dumps(payload))+')\n');calendar.chmod(0o755)
audio=binpath/'wpctl';audio.write_text('#!'+sys.executable+'\nimport json,sys\nwith open('+repr(str(actions))+',"a")as f:f.write(json.dumps(dict(provider="audio",args=sys.argv[1:]))+"\\n")\n');audio.chmod(0o755)
helper=assets/'system-controls.py';before=helper.read_bytes();helper.write_text('import json,sys\nr=sys.argv[1:]\nassert r==["displays"],"Brightness write prohibited"\nprint('+repr(json.dumps(dict(displays=[dict(bus='private',name='Private display with a long descriptive connection name',level=.5,maximum=100)])))+')\n');(out/'provider-seams.json').write_text(json.dumps(dict(path='system-controls.py',beforeSha256=hashlib.sha256(before).hexdigest(),afterSha256=hashlib.sha256(helper.read_bytes()).hexdigest(),audioExecutableSha256=hashlib.sha256(audio.read_bytes()).hexdigest(),calendarExecutableSha256=hashlib.sha256(calendar.read_bytes()).hexdigest(),scope='Synthetic private reads and recorded audio argv only; calendar mutations rejected; brightness writes prohibited')))
qml=out/'fixture.qml';qml.write_text('''import QtQuick
import QtQuick.Controls
import QtQuick.Window
import Quickshell
import Quickshell.Io
import "desktop" as D
ShellRoot {
 id:audit
 property var owner:window
 property string kind:""
 property string wanted:""
 FloatingWindow {id:window;title:"Shared Controls";visible:false;implicitWidth:300;implicitHeight:400
  Rectangle {id:surface;anchors.fill:parent;color:D.Theme.colors.background
   Column {x:16;y:16;width:parent.width-32;spacing:12
    D.AudioDeviceControl {id:audio;visible:audit.kind==="Audio";width:parent.width;node:({id:1,nickname:"Private playback device",audio:{volume:.5,muted:false}})}
    D.BrightnessControl {id:brightness;visible:audit.kind==="Brightness";width:parent.width}
    D.DateTimeField {id:date;visible:audit.kind==="Date";width:parent.width;label:"Starts";text:"2026-09-28T23:59"}
   }
   Shortcut {sequence:"Escape";onActivated:window.visible=false}
  }
 }
 property var returnActions:[]
 property int returnPointerPresses:0
 FloatingWindow {id:returns;title:"Return eligibility";visible:false;implicitWidth:420;implicitHeight:190
  Rectangle {anchors.fill:parent;color:D.Theme.colors.background
   TextField {id:returnInput;onPressed:++audit.returnPointerPresses;x:16;y:16;width:parent.width-32;Accessible.name:"Default input";font.family:D.Theme.font;font.pixelSize:D.Theme.textRole("body")}
   Row {x:16;y:80;spacing:8
    D.DialogButton {id:returnCancel;text:"Cancel";onClicked:audit.returnActions=audit.returnActions.concat("cancel")}
    D.DialogButton {id:returnOther;text:"Other";onClicked:audit.returnActions=audit.returnActions.concat("other")}
    D.DialogButton {id:returnPrimary;text:"Proceed";primary:true;onClicked:audit.returnActions=audit.returnActions.concat("primary")}
   }
  }
 }
 QtObject {id:stub;property date currentDate:new Date(2026,8,28);function claimPopup(name){}function focusOwnedWindow(name){}function openSettings(name){}}
 D.CalendarWindow {id:calendar;shell:stub}
 function walk(item,seen){if(!item || seen.includes(item))return [];seen.push(item);let result=[item];for(const child of Array.from(item.children || []).concat(Array.from(item.resources || [])))result=result.concat(walk(child,seen));if(item.contentItem)result=result.concat(walk(item.contentItem,seen));return result;}
 function shown(item){while(item && item!==owner.contentItem){if(!item.visible)return false;item=item.parent;}return item===owner.contentItem;}
 function name(item){while(item){if(typeof item.mapToItem==="function" && item.Accessible.name)return item.Accessible.name;item=item.parent;}return "";}
 function rect(item){const p=item.mapToItem(owner.contentItem,0,0);return {x:p.x,y:p.y,width:item.width,height:item.height};}
 IpcHandler {target:"sharedTypography"
  function ready():bool {return true;}
  function open(value:string):void {returns.visible=false;window.visible=false;calendar.visible=false;audit.kind=value;audit.owner=value==="CalendarEditor"?calendar:window;owner.implicitWidth=value==="CalendarEditor"?480:300;owner.implicitHeight=value==="CalendarEditor"?320:400;owner.visible=true;Qt.callLater(()=>owner.contentItem.Window.window.requestActivate());}
  function returnCase(mode:string):void {returns.visible=false;window.visible=false;calendar.visible=false;audit.kind="ReturnSemantics";audit.owner=returns;returnInput.text="";audit.returnActions=[];audit.returnPointerPresses=0;returnPrimary.enabled=mode!=="disabled";returnPrimary.visible=mode!=="hidden";returns.visible=true;Qt.callLater(()=>returns.contentItem.Window.window.requestActivate());}
  function focusChild(value:string):void {const button=value==="Cancel"?returnCancel:returnPrimary;audit.wanted=value;button.contentItem.forceActiveFocus();}
  function edit():void {calendar.create(calendar.selected);}
  function focus(value:string):void {audit.wanted=value;const item=walk(owner.contentItem,[]).find(item=>typeof item.mapToItem==="function" && item.Accessible.name===value && shown(item));if(!item)throw Error("Visible control missing "+value);item.forceActiveFocus();}
  function state():string {const backing=owner.contentItem.Window.window;if(!backing)return JSON.stringify({visible:owner.visible,active:false,width:owner.width,height:owner.height,scale:D.Theme.textScale});const items=walk(owner.contentItem,[]),focus=owner.contentItem.Window.window.activeFocusItem,target=items.find(item=>typeof item.mapToItem==="function" && item.Accessible.name===audit.wanted && shown(item)),tip=walk(brightness,[]).find(item=>item.contentItem && item.padding===7 && typeof item.open==="function"),slider=walk(brightness,[]).find(item=>typeof item.mapToItem==="function" && item.Accessible.name==="Display brightness");let ancestry=[];for(let item=focus;item;item=item.parent){if(item===returnCancel)ancestry.push("cancel");if(item===returnOther)ancestry.push("other");if(item===returnPrimary)ancestry.push("primary");}return JSON.stringify({returnPointerPresses:audit.returnPointerPresses,returnActions:audit.returnActions,input:returnInput.text,focusAncestry:ancestry,visible:owner.visible,active:owner.contentItem.Window.window.active,focus:name(focus),target:target?rect(target):null,targetText:target && target.contentItem && target.contentItem.font?{rect:rect(target.contentItem),color:String(target.contentItem.color)}:null,width:owner.width,height:owner.height,scale:D.Theme.textScale,date:date.text,loaded:calendar.calendarState.loaded,busy:calendar.calendarState.busy,editing:calendar.calendarState.editing,tooltip:tip?{opened:tip.opened,text:tip.text,color:String(tip.contentItem.color),pixelSize:tip.contentItem.font.pixelSize,rect:tip.opened?rect(tip.contentItem):null}:null,slider:audit.kind==="Brightness" && slider?rect(slider):null});}
 }
}''')
config=out/'sway.conf';config.write_text('include /source/res/lumen-desktop/sway.conf\noutput HEADLESS-1 resolution 900x700\noutput * bg #29445b solid_color\nfocus_follows_mouse no\n');processes=[];keyboard=None;pointer=None;records=[];phases=[]
def launch(command,name,**kwargs):
 child_env=kwargs.pop("child_env",env)
 with (out/name).open('w')as log:child=sp.Popen(list(map(str,command)),env=child_env,stdout=log,stderr=sp.STDOUT,start_new_session=True,**kwargs)
 processes.append(child);return child
def call(*command):return sp.check_output(list(map(str,command)),env=env,text=True,timeout=5).strip()
def ipc(method,*args):return call('quickshell','ipc','-p',qml,'call','sharedTypography',method,*args)
def state():return json.loads(ipc('state'))
def mark_phase(name):
 with (out/'shell.log').open('rb')as stream:text=stream.read(8*1024*1024).decode(errors='replace')
 previous=phases[-1]['shellLogBytes']if phases else 0;phases.append(dict(name=name,monotonic=time.monotonic(),shellLogBytes=len(text.encode()),newDiagnostics=text.encode()[previous:].decode(errors='replace')));(out/'phases.json').write_text(json.dumps(phases,indent=2))
def nodes(row):
 yield row
 for child in row.get('nodes',[])+row.get('floating_nodes',[]):yield from nodes(child)
def presented(name,title,subject):
 from PIL import Image,ImageColor
 receipts=[]
 def painted():
  tree=json.loads(call(a.sway.parent/'swaymsg','-r','-t','get_tree'));node=next(row for row in nodes(tree)if row.get('pid')==shell.pid and row.get('name')==title);origin=(node['rect']['x']+node['window_rect']['x'],node['rect']['y']+node['window_rect']['y']);r=subject['rect'];box=tuple(round(v)for v in (origin[0]+r['x'],origin[1]+r['y'],origin[0]+r['x']+r['width'],origin[1]+r['y']+r['height']));path=out/(name+'.png');call('grim','-o','HEADLESS-1',path)
  with Image.open(path)as image:ink=sum(max(abs(a-b)for a,b in zip(pixel,ImageColor.getrgb(subject['color'])))<=2 for pixel in image.convert('RGB').crop(box).get_flattened_data())
  receipts[:]=[dict(conId=node['id'],pid=node['pid'],title=title,box=box,inkPixels=ink)];(out/(name+'-paint.json')).write_text(json.dumps(receipts,indent=2));return ink>=4
 wait(painted,'actual '+name+' presented ink');return receipts[0]
def writes(provider):return [r for r in (map(json.loads,actions.read_text().splitlines())if actions.exists()else[])if r['provider']==provider]
def wait(predicate,label):
 deadline=time.monotonic()+15
 while time.monotonic()<deadline:
  if a.protocol_trace and (out/'shell.log').exists():assert (out/'shell.log').stat().st_size<=8*1024*1024,'Private protocol trace exceeded cooperative8MiB cap'
  try:
   if predicate():return
  except (ValueError,sp.SubprocessError):pass
  time.sleep(.03)
 try:last=state()
 except (ValueError,sp.SubprocessError)as error:last=dict(observerError=str(error))
 (out/'failure.json').write_text(json.dumps(dict(label=label,state=last,actions=actions.read_text()if actions.exists()else''),indent=2));raise AssertionError(label)
try:
 start_private_bus(env,out,processes);launch([a.sway,'-c',config],'sway.log');runtime=Path(env['XDG_RUNTIME_DIR']);wait(lambda:next(runtime.glob('sway-ipc*.sock'),None),'private compositor');env['SWAYSOCK']=str(next(runtime.glob('sway-ipc*.sock')));env['WAYLAND_DISPLAY']=next(path.name for path in runtime.glob('wayland-*')if not path.name.endswith('.lock'));shell=launch(['quickshell','--no-duplicate','-p',qml],'shell.log',child_env=dict(env,WAYLAND_DEBUG='client')if a.protocol_trace else env)
 with (out/'keyboard.log').open('w')as log:keyboard=PhysicalKeyboard(a.keyboard,env,log)
 processes.append(keyboard.process);pointer=launch([a.pointer_tool,'pointer','move','0','0'],'pointer.log',stdin=sp.PIPE,text=True);wait(lambda:ipc('ready')=='true','private IPC readiness');mark_phase('IPC ready')
 ipc('open','Audio');wait(lambda:state()['active']and state()['scale']==1.5,'audio window and actual text preference active');ipc('focus','Private playback device volume');wait(lambda:state()['focus']=='Private playback device volume','actual volume focus');keyboard.key('-k','Right');wait(lambda:len(writes('audio'))==1,'private actual slider adapter write');assert writes('audio')[0]['args'][:4]==['set-volume','-l','1','1'];keyboard.key('-k','Tab');wait(lambda:state()['focus']=='Mute Private playback device','physical Tab to Mute');keyboard.key('-k','space');wait(lambda:len(writes('audio'))==2,'private actual Mute adapter write');assert writes('audio')[-1]['args']==['set-mute','1','1'];records.append(dict(kind='Audio',state=state(),writes=writes('audio')));mark_phase('Audio complete')
 ipc('open','Date');wait(lambda:state()['active'],'date window active');ipc('focus','Starts year');wait(lambda:state()['focus']=='Starts year','actual date year focus');keyboard.key('-k','Up');wait(lambda:state()['date'].startswith('2027-'),'physical numeric step commits exact date');records.append(dict(kind='Date',state=state()));keyboard.key('-k','Escape');wait(lambda:not state()['visible'],'private control wrapper dismissal');mark_phase('Date complete')
 for label,focus,expected,child in [('input','Default input','primary',False),('cancel','Cancel','cancel',False),('cancel-child','Cancel','cancel',True),('other','Other','other',False),('primary','Proceed','primary',False),('primary-child','Proceed','primary',True),('disabled','Default input',None,False),('hidden','Default input',None,False)]:
  ipc('returnCase',label);wait(lambda:state()['active'],'actual return-case window '+label);ipc('focusChild'if child else'focus',focus);wait(lambda:state()['focus']==focus,'actual return recipient '+label)
  if focus!='Default input':assert expected in state()['focusAncestry'],state()
  if label=='cancel-child':call('grim','-o','HEADLESS-1',out/'focused-cancel-child.png')
  keyboard.key('-k','Return')
  if expected:wait(lambda:len(state()['returnActions'])>0,'actual Return action '+label);assert state()['returnActions']==[expected],state()
  else:keyboard.type('z');wait(lambda:state()['input']=='z','ordered post-Return input receipt '+label);assert state()['returnActions']==[],state()
  records.append(dict(kind='ReturnSemantics',case=label,state=state()));mark_phase('Return '+label)
 ipc('open','CalendarEditor');wait(lambda:state()['active']and state()['loaded']and not state()['busy'],'actual calendar parent ready');ipc('edit');wait(lambda:state()['focus']=='Event title','actual editor initial focus');ipc('focus','Starts year');wait(lambda:state()['focus']=='Starts year','actual editor date recipient');keyboard.key('-k','Up');ipc('focus','Cancel');wait(lambda:state()['focus']=='Cancel','actual constrained footer recipient');row=state();r=row['target'];assert r['x']>=0 and r['y']>=0 and r['x']+r['width']<=row['width']+1 and r['y']+r['height']<=row['height']+1,row;editor_paint=presented('editor','Calendar',row['targetText']);before=len(writes('calendar'));keyboard.key('-k','Return');wait(lambda:(not state()['editing']and not state()['busy'])or any(r['request'].get('action')in('create','update','delete')for r in writes('calendar')[before:]),'actual Cancel completion');assert not any(r['request'].get('action')in('create','update','delete')for r in writes('calendar')),writes('calendar');assert not state()['editing'];records.append(dict(kind='CalendarEditor',state=state(),painted=editor_paint));keyboard.key('-k','Escape');mark_phase('Calendar complete')
 ipc('open','Brightness');wait(lambda:state()['active']and state()['slider'],'actual brightness recipient');ipc('focus','Display brightness');wait(lambda:state()['focus']=='Display brightness','actual brightness keyboard focus')
 tree=json.loads(call(a.sway.parent/'swaymsg','-r','-t','get_tree'));node=next(row for row in nodes(tree)if row.get('pid')==shell.pid and row.get('name')=='Shared Controls');origin=(node['rect']['x']+node['window_rect']['x'],node['rect']['y']+node['window_rect']['y']);r=state()['slider'];x=origin[0]+r['x']+r['width']/2;y=origin[1]+r['y']+r['height']/2;call(a.sway.parent/'swaymsg','-r','seat seat0 cursor set '+str(round(x))+' '+str(round(y)));pointer.stdin.write('m 0 0\n');pointer.stdin.flush();wait(lambda:state()['tooltip']and state()['tooltip']['opened'],'actual pointer hover tooltip');row=state();r=row['tooltip']['rect'];assert r['x']>=0 and r['y']>=0 and r['x']+r['width']<=row['width']+1 and r['y']+r['height']<=row['height']+1,row;assert row['tooltip']['pixelSize']==17,row;brightness_paint=presented('brightness','Shared Controls',row['tooltip']);records.append(dict(kind='Brightness',state=row,painted=brightness_paint));keyboard.key('-k','Escape');wait(lambda:not state()['visible'],'brightness wrapper dismissal');mark_phase('Brightness complete')
 ipc('returnCase','reconnect');wait(lambda:state()['active'],'reconnect target active');ipc('focus','Default input');wait(lambda:state()['focus']=='Default input','reconnect keyboard recipient')
 keyboard_enters=len(re.findall(r'wl_keyboard#\d+\.enter\(', (out/'shell.log').read_text()))
 keyboard.close();wait(lambda:not any(item['type']=='keyboard'for item in json.loads(call(a.sway.parent/'swaymsg','-r','-t','get_inputs'))),'keyboard capability removed');mark_phase('Keyboard absent')
 with (out/'keyboard-reconnected.log').open('w')as log:keyboard=PhysicalKeyboard(a.keyboard,env,log)
 processes.append(keyboard.process)
 wait(lambda:len(re.findall(r'wl_keyboard#\d+\.enter\(', (out/'shell.log').read_text()))>keyboard_enters and state()['active'] and state()['focus']=='Default input','client bound recreated keyboard and acknowledged UI focus')
 keyboard.type('reconnected');wait(lambda:state()['input']=='reconnected','new keyboard reaches retained focus');records.append(dict(kind='KeyboardReconnect',state=state()));mark_phase('Keyboard reconnected')
 pointer_enters=len(re.findall(r'wl_pointer#\d+\.enter\(', (out/'shell.log').read_text()))
 pointer.stdin.close();pointer.wait(timeout=5);wait(lambda:not any(item['type']=='pointer'for item in json.loads(call(a.sway.parent/'swaymsg','-r','-t','get_inputs'))),'pointer capability removed');mark_phase('Pointer absent')
 pointer=launch([a.pointer_tool,'pointer','move','0','0'],'pointer-reconnected.log',stdin=sp.PIPE,text=True)
 row=state();tree=json.loads(call(a.sway.parent/'swaymsg','-r','-t','get_tree'));node=next(item for item in nodes(tree)if item.get('pid')==shell.pid and item.get('name')=='Return eligibility');r=row['target'];x=node['rect']['x']+node['window_rect']['x']+r['x']+r['width']/2;y=node['rect']['y']+node['window_rect']['y']+r['y']+r['height']/2
 call(a.sway.parent/'swaymsg','-r','seat seat0 cursor set '+str(round(x))+' '+str(round(y)));pointer.stdin.write('m 0 0\n');pointer.stdin.flush()
 wait(lambda:len(re.findall(r'wl_pointer#\d+\.enter\(', (out/'shell.log').read_text()))>pointer_enters and state()['active'],'client bound recreated pointer and acknowledged active window')
 pointer.stdin.write('p\nr\n');pointer.stdin.flush();wait(lambda:state()['returnPointerPresses']==1,'new pointer actual field press');keyboard.type(' pointer');wait(lambda:state()['input']=='reconnected pointer','reconnected pointer preserves real field focus');assert state()['returnActions']==[],state();records.append(dict(kind='PointerReconnect',state=state()));mark_phase('Pointer reconnected')
 ipc('open','Brightness');wait(lambda:state()['active'],'final private window active');keyboard.key('-k','Escape');wait(lambda:not state()['visible'],'final private window dismissed')
 assert not re.search(r'ReferenceError|TypeError|binding loop',(out/'shell.log').read_text())
 (out/'result.json').write_text(json.dumps(dict(passed=True,cases=records,runtimeWarnings=[line for line in (out/'shell.log').read_text().splitlines()if 'WARN'in line],scope='Actual shared control input and parent editor cancel; no host provider or device'),indent=2))
finally:
 if (out/'shell.log').exists():mark_phase('Before private input teardown')
 if keyboard:keyboard.close()
 if (out/'shell.log').exists():mark_phase('After private keyboard close')
 if pointer and pointer.stdin and not pointer.stdin.closed:pointer.stdin.close()
 for child in reversed(processes):
  if child.poll()is None:
   os.killpg(child.pid,signal.SIGTERM)
   try:child.wait(timeout=5)
   except sp.TimeoutExpired:os.killpg(child.pid,signal.SIGKILL);child.wait(timeout=5)
 close_reaped_pipes(processes)
 if (out/'shell.log').exists():
  mark_phase('All private children reaped');text=(out/'shell.log').read_text();result=out/'result.json'
  if result.exists():
   data=json.loads(result.read_text());data['runtimeWarnings']=[line for line in text.splitlines()if 'WARN'in line];data['protocolTrace']=dict(enabled=a.protocol_trace,bytes=(out/'shell.log').stat().st_size,cooperativeCapBytes=8*1024*1024)
   if a.protocol_trace:
    from application_fixture.seat_protocol import inspect
    data['seatProtocol']=inspect(text)
    data['passed']=data['passed'] and not data['seatProtocol']['errors'] and all(data['seatProtocol']['counts'][kind]['enter'] and data['seatProtocol']['counts'][kind]['leave'] for kind in ('keyboard','pointer'))
   data['passed']=data['passed'] and not data['runtimeWarnings'];result.write_text(json.dumps(data,indent=2));assert data['passed'],'Private focus ordering or runtime warning failure; inspect retained result'
  if a.protocol_trace:assert (out/'shell.log').stat().st_size<=8*1024*1024,'Private protocol trace exceeded cooperative8MiB cap'

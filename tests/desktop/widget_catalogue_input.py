"""Real gallery clicks, masked widgets, staged movement and restart on two outputs."""
import argparse,json,os,re,shutil,signal,subprocess as sp,sys,tempfile,time
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--output',type=Path,required=True);p.add_argument('--launcher',type=Path,required=True);p.add_argument('--sway',required=True);p.add_argument('--pointer-tool',required=True);p.add_argument('--theme',choices=['light','dark'],default='dark');p.add_argument('--inside',action='store_true');p.add_argument('--effects',action='store_true');a=p.parse_args()
if not a.inside:
 with tempfile.TemporaryDirectory(prefix='lumen-widgets-') as private:
  env=dict(os.environ,HOME=private,XDG_RUNTIME_DIR=private,DBUS_SYSTEM_BUS_ADDRESS='unix:path='+private+'/absent-system')
  for key in ('CONFIG','STATE','CACHE','DATA'):env['XDG_'+key+'_HOME']=str(Path(private)/key.lower())
  for key in ('DISPLAY','WAYLAND_DISPLAY','SWAYSOCK','DBUS_SESSION_BUS_ADDRESS','AT_SPI_BUS_ADDRESS'):env.pop(key,None)
  config=Path(private)/'bus.conf';config.write_text('<busconfig><type>session</type><listen>unix:tmpdir='+private+'</listen><auth>EXTERNAL</auth><policy context="default"><allow send_destination="*"/><allow receive_sender="*"/><allow own="*"/></policy></busconfig>')
  raise SystemExit(sp.call(['dbus-run-session','--config-file='+str(config),'--',sys.executable,__file__,*sys.argv[1:],'--inside'],env=env))
repo=Path(__file__).resolve().parents[2];out=a.output.resolve();out.mkdir(parents=True,exist_ok=False)
prefix=re.split(r'\n[ \t]*exec quickshell ',a.launcher.read_text(),maxsplit=1)[0]
env=dict(value.decode().split('=',1) for value in sp.check_output(['bash','-c',prefix+'\nexec env -0']).split(b'\0') if b'=' in value)
env.update(WLR_BACKENDS='headless',WLR_HEADLESS_OUTPUTS='2',WLR_RENDERER='gles2' if a.effects else 'pixman',WLR_RENDERER_FORCE_SOFTWARE='1' if a.effects else '0',WLR_RENDERER_ALLOW_SOFTWARE='1',LIBGL_ALWAYS_SOFTWARE='1',WLR_LIBINPUT_NO_DEVICES='1',QT_QPA_PLATFORM='wayland',QT_QPA_PLATFORMTHEME='generic',QT_NO_XDG_DESKTOP_PORTAL='1',QT_QUICK_BACKEND='software',QT_ACCESSIBILITY='0',QT_LINUX_ACCESSIBILITY_ALWAYS_ON='0',NO_AT_BRIDGE='1',GTK_A11Y='none',LUMEN_NATIVE_WINDOW_CONTROLS='0',LUMEN_NATIVE_WINDOW_ACTIONS='0',LUMEN_FILE_SERVICE='0',LUMEN_LAUNCH_SCOPES='0')
for key in ('DISPLAY','WAYLAND_DISPLAY','SWAYSOCK','AT_SPI_BUS_ADDRESS'):env.pop(key,None)
shutil.copytree(repo/'res/lumen-desktop',out/'assets');palettes=json.loads((repo/'res/lumen-desktop/appearance/palette.json').read_text())
appearance=out/'appearance.json';appearance.write_text(json.dumps(dict(font=palettes['font'],palette=palettes[a.theme],preferences={})));env['LUMEN_APPEARANCE_STATE']=str(appearance)
qml=out/'shell.qml';qml.write_text('''import QtQuick
import QtQuick.Controls
import QtQuick.Window
import Quickshell
import Quickshell.Io
import Quickshell.Wayland
import "assets/shell" as D
ShellRoot {
 id:fixture
 property int activations:0
 property int backgroundClicks:0
 property var captured:null
 property var capturedEditor:null
 property alias widgetService:widgets
 D.WidgetState {id:widgets}
 QtObject {id:menu;property var state:({windowId:0})}
 QtObject {id:stub;property var widgets:fixture.widgetService;property var exportedMenu:menu;property var widgetSurfaces:({});property date currentDate:new Date(2026,9,19,13,5);function showMenu(kind,output){fixture.activations++;}function openSettings(page){}function openCalendarAt(day,event){fixture.activations++;}}
 Variants {model:Quickshell.screens
  Scope {required property var modelData
   PanelWindow {screen:modelData;anchors {top:true;bottom:true;left:true;right:true} color:"#303030";exclusiveZone:0;WlrLayershell.layer:WlrLayer.Background;MouseArea {anchors.fill:parent;onClicked:fixture.backgroundClicks++}}
   D.DesktopWidgets {screen:modelData;outputName:modelData.name;shell:stub}
  }
 }
 FloatingWindow {id:window;title:"Widget Catalogue";visible:true;implicitWidth:420;implicitHeight:680
  Rectangle {id:body;anchors.fill:parent;color:D.Theme.colors.background
   D.FocusFlickable {id:scroll;anchors.fill:parent;anchors.margins:20;contentWidth:width;contentHeight:editor.implicitHeight;clip:true;D.DesktopWidgetEditor {id:editor;width:scroll.width;service:widgets}}
  }
 }
 function find(item,name){if(item.objectName===name)return item;for(const child of item.children || []){const result=find(child,name);if(result)return result;}return null;}
 IpcHandler {target:"widgetAudit"
  function state():string{return JSON.stringify({preferences:widgets.preferences,error:widgets.error,outputs:Object.keys(stub.widgetSurfaces).filter(name=>stub.widgetSurfaces[name]).map(name=>({name:name,items:stub.widgetSurfaces[name].status()})),activations:fixture.activations,background:fixture.backgroundClicks,scrollY:scroll.contentY,focused:window.activeFocusItem?window.activeFocusItem.objectName:"",sameEditor:!fixture.capturedEditor || widgets.desktopItems.length>0 && find(body,"widget-size-"+widgets.desktopItems[0].id)===fixture.capturedEditor,same:!fixture.captured || widgets.desktopItems.length>0 && stub.widgetSurfaces["HEADLESS-1"].findWidget(widgets.desktopItems[0].id)===fixture.captured});}
  function point(name:string):string{const item=find(body,name);if(!item)throw Error("Missing "+name);const p=item.mapToItem(body,item.width/2,item.height/2);return JSON.stringify({x:p.x,y:p.y});}
  function widgetPoint(name:string):string{const card=stub.widgetSurfaces["HEADLESS-1"].findWidget(widgets.desktopItems[0].id);const target=find(card,name);if(!target)throw Error("Missing widget control "+name);const p=target.mapToItem(card,target.width/2,target.height/2);return JSON.stringify({x:card.x+p.x,y:card.y+p.y});}
  function capture():void{fixture.capturedEditor=find(body,"widget-size-"+widgets.desktopItems[0].id);fixture.captured=stub.widgetSurfaces["HEADLESS-1"].findWidget(widgets.desktopItems[0].id);}
 }
}''')
processes=[]
def launch(command,log):
 with (out/log).open('w') as stream:process=sp.Popen(command,env=env,stdout=stream,stderr=sp.STDOUT,start_new_session=True)
 processes.append(process);return process
def call(*command):return sp.check_output(command,env=env,text=True,timeout=5).strip()
def wait(predicate):
 deadline=time.monotonic()+15
 while time.monotonic()<deadline:
  try:
   result=predicate()
   if result:return result
  except (OSError,ValueError,KeyError,sp.SubprocessError):pass
  time.sleep(.04)
 raise AssertionError('Widget input timeout; inspect '+str(out)+'; compositor logs: '+ '\n'.join(path.name+': '+path.read_text() for path in out.glob('*sway*.log')))
def ipc(method,*args):return call('quickshell','-p',str(qml),'ipc','call','widgetAudit',method,*args)
def state():
 value=json.loads(ipc('state'));(out/'last-state.json').write_text(json.dumps(value,indent=2));return value
def nodes(node):
 yield node
 for child in node.get('nodes',[])+node.get('floating_nodes',[]):yield from nodes(child)
def point(name):
 local=json.loads(ipc('point',name));window=next(node for node in nodes(json.loads(call('swaymsg','-r','-t','get_tree'))) if node.get('name')=='Widget Catalogue');return local['x']+window['rect']['x'],local['y']+window['rect']['y']
def move(x,y):
 call('swaymsg','seat','seat0','cursor','set',str(round(x)),str(round(y)));pointer.stdin.write('m 0.5 0.5\n');pointer.stdin.flush();time.sleep(.12)
def command(value):pointer.stdin.write(value+'\n');pointer.stdin.flush();time.sleep(.12)
def click(position):move(*position);command('p');command('r')
def key(*values):call('wtype',*values);time.sleep(.12)
def card():return next(output['items'][0] for output in state()['outputs'] if output['name']=='HEADLESS-1' and output['items'])
def item():return state()['preferences']['desktopItems'][0]
try:
 config=out/'sway.conf';config.write_text('output HEADLESS-1 resolution 1280x900 position 0 0\noutput HEADLESS-2 resolution 1280x900 position 1280 0\nxwayland disable\nfocus_follows_mouse no\nfor_window [all] floating enable, border pixel 0\n')
 launch([a.sway,'--unsupported-gpu','-c',str(config)],'sway.log');runtime=Path(env['XDG_RUNTIME_DIR']);env['SWAYSOCK']=str(wait(lambda:next(runtime.glob('sway-ipc*.sock'),None)));env['WAYLAND_DISPLAY']=wait(lambda:next((path.name for path in runtime.glob('wayland-*') if not path.name.endswith('.lock')),None))
 shell=launch(['quickshell','-p',str(qml)],'shell.log');wait(lambda:len(state()['outputs'])==2)
 document=out/'synthetic-document.txt'
 document_process=launch(['foot','--app-id=lumen-recovery-document','--title=Recovery document','-e','sh','-c','cat > "$1"','fixture',str(document)],'document.log')
 def document_nodes():return [node for node in nodes(json.loads(call('swaymsg','-r','-t','get_tree'))) if node.get('app_id')=='lumen-recovery-document']
 document_node=wait(lambda:next(iter(document_nodes()),None));document_identity=(document_node['id'],document_node['pid'])
 call('swaymsg','[con_id='+str(document_identity[0])+']','move container to workspace recovery-document')
 call('swaymsg','workspace','1')

 launch(['wtype','-s','60000'],'keyboard.log');pointer=sp.Popen([a.pointer_tool,'pointer','move','0','0'],env=env,stdin=sp.PIPE,text=True,start_new_session=True);processes.append(pointer);time.sleep(.2)
 click(point('widget-add-clock'));wait(lambda:state()['preferences'].get('desktopItems') and all(len(output['items'])==1 for output in state()['outputs']))
 checks=['Actual catalogue click creates one stable instance on both outputs']
 ipc('capture');clock=card();click((clock['x']+90,clock['y']+70));wait(lambda:state()['activations']==1);checks.append('Clock activation emits exactly one related-content action')
 click((20,850));wait(lambda:state()['background']==1);checks.append('Widget union mask lets desktop background clicks through')
 clock=card();start=(clock['x']+clock['width']-25,clock['y']+14);move(*start);command('p');move(start[0]+100,start[1]+60);wait(lambda:card()['moving']);key('-k','Escape');command('r');wait(lambda:not card()['moving']);assert not item()['positions'];checks.append('Pointer preview and Escape do not persist a cancelled placement')
 clock=card();start=(clock['x']+clock['width']-25,clock['y']+14);move(*start);command('p');move(start[0]+100,start[1]+60);command('r');wait(lambda:'HEADLESS-1' in item()['positions']);assert 'HEADLESS-2' not in item()['positions'];assert state()['same'];checks.append('Pointer release saves only the touched display without recreating the widget')
 original=dict(item()['positions']['HEADLESS-1']);key('-k','space','-k','Right','-k','Escape');assert item()['positions']['HEADLESS-1']==original
 key('-k','space','-k','Right','-k','Return');wait(lambda:item()['positions']['HEADLESS-1']['x']>original['x']);checks.append('Keyboard move previews cancel or commit against the same captured instance')
 # Kill the actual shell while a native move owns the pointer. The last committed
 # layout survives; preview coordinates and input grabs belong to the dead process.
 saved_before_crash=json.loads(json.dumps(item()));clock=card();start=(clock['x']+clock['width']-25,clock['y']+14)
 move(*start);command('p');move(start[0]+80,start[1]+40);wait(lambda:card()['moving'])
 os.kill(shell.pid,signal.SIGKILL);shell.wait(timeout=5);command('r')
 shell=launch(['quickshell','-p',str(qml)],'shell-crash-restart.log')
 wait(lambda:state()['preferences'].get('desktopItems') and item()['id']==saved_before_crash['id'])
 assert item()==saved_before_crash and not card()['moving']
 assert [(node['id'],node['pid']) for node in document_nodes()]==[document_identity]
 call('swaymsg','[con_id='+str(document_identity[0])+']','focus')
 key('recovery-document-still-editable','-k','Return')
 wait(lambda:document.exists() and 'recovery-document-still-editable' in document.read_text())
 call('swaymsg','workspace','1')

 click((20,850));wait(lambda:state()['background']==1)
 clock=card();click((clock['x']+90,clock['y']+70));wait(lambda:state()['activations']==1)
 time.sleep(.2);assert state()['activations']==1
 ipc('capture');checks.append('SIGKILL during held native movement preserves committed layout, surviving editable document identity, click-through and one fresh activation')
 identifier=item()['id'];click(point('widget-size-'+identifier));key('-k','Home','-k','Return');wait(lambda:item()['size']=='small' and card()['width']==214);assert state()['same'] and state()['sameEditor'];checks.append('Size control updates both surfaces while preserving component identity')
 call('grim',str(out/('widgets-'+a.theme+'.png')))
 saved=dict(item());os.killpg(shell.pid,signal.SIGTERM);shell.wait(timeout=5);shell=launch(['quickshell','-p',str(qml)],'shell-restart.log');wait(lambda:state()['preferences'].get('desktopItems') and item()['id']==saved['id']);assert item()==saved;checks.append('Restart restores the exact instance, size and independent positions')
 click(point('widget-remove-'+identifier));wait(lambda:state()['preferences']['desktopItems']==[] and all(not output['items'] for output in state()['outputs']));checks.append('Removing the last instance preserves an explicitly empty layout')
 click(point('widget-add-calendar'));wait(lambda:item()['type']=='calendar' and all(len(output['items'])==1 for output in state()['outputs']));assert item()['id']!=identifier;checks.append('Calendar can be added after an explicitly empty layout with a fresh identity')
 call('grim',str(out/('calendar-'+a.theme+'.png')))
 target=json.loads(ipc('widgetPoint','calendar-open-related'));click((target['x'],target['y']));wait(lambda:state()['activations']==1);key('-k','space');wait(lambda:state()['activations']==2);time.sleep(.2);assert state()['activations']==2;checks.append('Calendar related-content pointer and keyboard activation each dispatch exactly once')
 for index in range(3):click(point('widget-add-clock'))
 maximumScroll=0
 for index in range(24):
  key('-k','Tab');maximumScroll=max(maximumScroll,state()['scrollY'])
 assert maximumScroll>0;checks.append('Keyboard traversal reveals controls below the widget editor viewport')
 assert not state()['error'],state()
 for name in ('shell.log','shell-restart.log','shell-crash-restart.log'):
  text=(out/name).read_text();assert not any(marker in text for marker in ('TypeError:','ReferenceError:','Binding loop','Failed to load configuration','Cannot assign')),text
 (out/'result.json').write_text(json.dumps(dict(passed=True,checks=checks,final=state(),scope='Two private native outputs, synthetic date, real pointer and keyboard; no live accounts/settings'),indent=2)+'\n')
finally:
 for process in reversed(processes):
  if process.poll() is None:os.killpg(process.pid,signal.SIGTERM)
 for process in reversed(processes):
  try:process.wait(timeout=3)
  except sp.TimeoutExpired:os.killpg(process.pid,signal.SIGKILL);process.wait(timeout=3)
print('PASS native widget catalogue, mask, captured movement, resize and restart')

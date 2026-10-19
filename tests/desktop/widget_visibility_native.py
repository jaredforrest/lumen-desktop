"""Widget visibility and rendering under actual private compositor focus."""
import argparse,json,os,re,shutil,signal,subprocess as sp,sys,tempfile,time
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--output',type=Path,required=True);p.add_argument('--launcher',type=Path,required=True);p.add_argument('--sway',required=True);p.add_argument('--pointer-tool',required=True);p.add_argument('--effects',action='store_true');p.add_argument('--inside',action='store_true');a=p.parse_args()
if not a.inside:
    with tempfile.TemporaryDirectory(prefix='lumen-device-input-',ignore_cleanup_errors=True) as private:
        env=dict(os.environ,HOME=private,XDG_RUNTIME_DIR=private)
        for key in ('CONFIG','STATE','CACHE','DATA'):env['XDG_'+key+'_HOME']=str(Path(private)/key.lower())
        for key in ('DISPLAY','WAYLAND_DISPLAY','SWAYSOCK','DBUS_SESSION_BUS_ADDRESS','AT_SPI_BUS_ADDRESS'):env.pop(key,None)
        config=Path(private)/'bus.conf'
        config.write_text('<busconfig><type>session</type><listen>unix:tmpdir='+private+'</listen><auth>EXTERNAL</auth><policy context="default"><allow send_destination="*"/><allow receive_sender="*"/><allow own="*"/></policy></busconfig>')
        raise SystemExit(sp.call(['dbus-run-session','--config-file='+str(config),'--',sys.executable,__file__,*sys.argv[1:],'--inside'],env=env))
repo=Path(__file__).resolve().parents[2];sys.path.insert(0,str(Path(__file__).parent/'fixtures'))
from input_ipc import settings_adapter
out=a.output.resolve();out.mkdir(parents=True,exist_ok=False)
prefix=re.split(r'\n[ \t]*exec quickshell ',a.launcher.read_text(),maxsplit=1)[0]
exported=sp.check_output(['bash','-c',prefix+'\nexec env -0']);env=dict(value.decode().split('=',1) for value in exported.split(b'\0') if b'=' in value)
env.update(WLR_BACKENDS='headless',WLR_RENDERER='gles2' if a.effects else 'pixman',WLR_RENDERER_ALLOW_SOFTWARE='1',LIBGL_ALWAYS_SOFTWARE='1',WLR_LIBINPUT_NO_DEVICES='1',QT_QPA_PLATFORM='wayland',QT_QPA_PLATFORMTHEME='generic',QT_QUICK_BACKEND='software',QT_ACCESSIBILITY='0',QT_LINUX_ACCESSIBILITY_ALWAYS_ON='0',NO_AT_BRIDGE='1',GTK_A11Y='none',LUMEN_FILE_SERVICE='0',LUMEN_LAUNCH_SCOPES='0',LUMEN_NATIVE_WINDOW_CONTROLS='0',LUMEN_NATIVE_WINDOW_ACTIONS='0',LUMEN_UNIFIED_OWNED_HEADERS='1')
for key in ('DISPLAY','WAYLAND_DISPLAY','SWAYSOCK','AT_SPI_BUS_ADDRESS'):env.pop(key,None)
palettes=json.loads((repo/'res/lumen-desktop/appearance/palette.json').read_text());appearance=out/'appearance.json';appearance.write_text(json.dumps(dict(font=palettes['font'],palette=palettes['dark'],preferences={})));env['LUMEN_APPEARANCE_STATE']=str(appearance)
shutil.copytree(repo/'res/lumen-desktop',out/'assets')
env.update(settings_adapter(out,env['PATH']))
from PIL import Image,ImageStat
from test_calendar_widget_events import populate
mirror=out/'mirror';populate(mirror);env['LUMEN_CALENDAR_TIMEZONE']='Europe/London';env['TZ']='Europe/London'
fake=out/'bin/lumen-calendar';fake.write_text('#!'+sys.executable+'\n'+"""import json,os,sys,time
from pathlib import Path
sys.path.insert(0,os.environ['LUMEN_CALENDAR_FIXTURE_SOURCE'])
import calendar_store
request=json.loads(sys.stdin.readline());assert request['action'] in ('list','load'), 'Read-only widget must not mutate or sync'
root=Path(os.environ['LUMEN_CALENDAR_FIXTURE'])
with (root/'requests.jsonl').open('a') as log:log.write(json.dumps(request)+'\\n')
if (root/'delay').exists():time.sleep(1.5)
if (root/'unavailable').exists():print(json.dumps(dict(error='Synthetic calendar provider unavailable')))
else:print(json.dumps(calendar_store.handle(dict(request,action='list'),root/'mirror')))
""");fake.chmod(0o755)
env['LUMEN_CALENDAR_FIXTURE_SOURCE']=str(repo/'res/lumen-desktop/shell');env['LUMEN_CALENDAR_FIXTURE']=str(out)
env['GLOBAL_MENU_ENABLED']='1';env['LUMEN_COMPOSITOR_EFFECTS']='1' if a.effects else '0';env['LUMEN_MENU_BRIDGE']=str(repo/'res/lumen-desktop/global-menu/bridge.py')
preferences=dict(desktopVisible=True,desktopColor='auto',reduceTransparency=False,desktopItems=[dict(id='calendar-'+size,type='calendar',size=size,style='month',output='HEADLESS-1',positions={}) for size in ('small','medium','large')]+[dict(id='clock-'+size,type='clock',size=size,style='month',hourCycle='24',output='HEADLESS-2',positions={}) for size in ('small','medium','large')])
settings=Path(env['XDG_STATE_HOME'])/'lumen-desktop/session/notification-centre.json';settings.parent.mkdir(parents=True,exist_ok=True);settings.write_text(json.dumps(preferences))
qml=out/'shell.qml';qml.write_text('''import QtQuick
import QtQuick.Window
import Quickshell
import Quickshell.Io
import Quickshell.Wayland
import "assets/shell" as D
ShellRoot {
 id:root
 property date currentDate:new Date(2026,10,1,13,5)
 property var actions:[]
 property int backgroundClicks:0
 property string bridgeError:""
 property int bridgeExit:-999
 property alias widgets:widgetState
 property alias exportedMenu:menu
 property var widgetSurfaces:({})
 D.WidgetState {id:widgetState}
 QtObject {id:menu;property var state:({windowId:0});property var latestState:state}
 function openCalendarAt(day,event){actions=actions.concat([{kind:event?"event":"related",id:event?event.id:null,day:Qt.formatDate(day,"yyyy-MM-dd")}]);}
 function showMenu(name,output){actions=actions.concat([{kind:"menu",output:output}]);}
 function openSettings(page){}
 Process {id:bridge;running:true;stdinEnabled:true;command:[Quickshell.env("GLOBAL_MENU_PYTHON"),Quickshell.env("LUMEN_MENU_BRIDGE")]
  stderr:StdioCollector {onStreamFinished:root.bridgeError=text}
  onExited:(code,status)=>root.bridgeExit=code
  stdout:SplitParser {onRead:line=>{try{const value=JSON.parse(line);menu.state=value;menu.latestState=value;}catch(error){console.warn("Invalid native menu state",line);}}}
 }
 Variants {model:Quickshell.screens
  Scope {required property var modelData
   PanelWindow {screen:modelData;anchors {top:true;left:true;right:true;bottom:true}color:"#256048";exclusiveZone:0;WlrLayershell.layer:WlrLayer.Background;WlrLayershell.keyboardFocus:WlrKeyboardFocus.OnDemand
    MouseArea {anchors.fill:parent;onClicked:root.backgroundClicks++}
   }
   D.DesktopWidgets {screen:modelData;outputName:modelData.name;shell:root}
  }
 }
 function find(name,item){if(!item)return null;if(item.objectName===name)return item;for(const child of item.children || []){const result=find(name,child);if(result)return result;}return null;}
 function item(id,output){const surface=root.widgetSurfaces[output];return surface?surface.findWidget(id):null;}
 function rows(){let result=[];for(const name of Object.keys(root.widgetSurfaces)){const surface=root.widgetSurfaces[name];for(const row of surface.status()){const widget=item(row.id,name);const material=find("widget-material",widget);const calendar=widget.descriptor.type==="calendar"?find("widget-calendar-content",widget):null;const related=find("calendar-open-related",widget);const at=related?related.mapToItem(surface.contentItem,0,0):null;result.push(Object.assign({},row,{output:name,screenX:surface.screen.x,screenY:surface.screen.y,visible:surface.visible,monochrome:widget.monochrome,opacity:widget.opacity,opaque:widget.reduceTransparency,alpha:material.color.a,selected:calendar?calendar.iso(calendar.selected):null,loaded:calendar?calendar.loaded:true,related:at?{x:at.x,y:at.y,width:related.width,height:related.height}:null}));}}return result;}
 IpcHandler {target:"filesAudit"
  function dayStyle():string {const cell=find("calendar-day-2026-11-01",item("calendar-medium","HEADLESS-1"));return JSON.stringify({foreground:String(cell.contentItem.color),background:String(cell.background.children[0].color),font:cell.contentItem.font.pixelSize});}
  function preference(key:string,value:string):void {widgetState.setPreference(key,JSON.parse(value));}
  function update(id:string,patch:string):void {widgetState.updateWidget(id,JSON.parse(patch));}
  function focusRelated(id:string,output:string):void {find("calendar-open-related",item(id,output)).forceActiveFocus();}
  function focusClock(id:string,output:string):void {find("widget-clock-content",item(id,output)).forceActiveFocus();}
  function state():string {return JSON.stringify({rows:rows(),bridgeRunning:bridge.running,bridgeError:root.bridgeError,bridgeExit:root.bridgeExit,windowId:menu.latestState.windowId,app:menu.latestState.appName,actions:root.actions,backgroundClicks:root.backgroundClicks,error:widgetState.error});}
  function geometry(id:string,output:string,name:string):string {const surface=root.widgetSurfaces[output];const widget=item(id,output);const target=name?find(name,widget):widget;if(!target)return "null";let ancestor=target.parent;while(name && ancestor){if(ancestor.contentY!==undefined && ancestor.contentItem){const pos=target.mapToItem(ancestor.contentItem,0,0);ancestor.contentY=Math.max(0,Math.min(ancestor.contentHeight-ancestor.height,pos.y-70));}ancestor=ancestor.parent;}const at=target.mapToItem(surface.contentItem,0,0);return JSON.stringify({x:at.x+surface.screen.x,y:at.y+surface.screen.y,width:target.width,height:target.height});}
 }
}''')
processes=[]
def launch(command,name):
    with (out/name).open('w') as log:
        process=sp.Popen(command,env=env,stdout=log,stderr=sp.STDOUT,start_new_session=True)
    processes.append(process);return process

def call(*command):return sp.check_output(command,env=env,text=True,timeout=10).strip()
def wait(fn):
    end=time.monotonic()+20
    while time.monotonic()<end:
        try:
            value=fn()
            if value:return value
        except (OSError,ValueError,StopIteration,sp.SubprocessError):pass
        time.sleep(.1)
    if (out/'shell.log').exists():
        try:(out/'failed-state.json').write_text(json.dumps(state(),indent=2))
        except Exception:pass
    raise AssertionError('Files input fixture timed out; inspect logs')
def nodes(node):
    yield node
    for child in node.get('nodes',[])+node.get('floating_nodes',[]):yield from nodes(child)
def window():return next(node for node in nodes(json.loads(call('swaymsg','-r','-t','get_tree'))) if node.get('app_id')=='org.quickshell' and node.get('name','')=='Calendar widget fixture')
def ipc(method,*values):return call('quickshell','-p',str(qml),'ipc','call','filesAudit',method,*values)
def state():return json.loads(ipc('state'))
def point(identifier,output,name='',rx=.5,ry=.5):
    value=json.loads(ipc('geometry',identifier,output,name));assert value,(identifier,name);return (value['x']+value['width']*rx,value['y']+min(value['height'],25)*ry)

def move(x,y):
    call('swaymsg','seat','seat0','cursor','set',str(round(x)),str(round(y)));pointer.stdin.write('m 0.5 0.5\n');pointer.stdin.flush();time.sleep(.12)
def command(value):pointer.stdin.write(value+'\n');pointer.stdin.flush();time.sleep(.12)
def click(position):move(*position);command('p');command('r')
def key(*keys):call('wtype',*keys);time.sleep(.15)
def drag(start,dx,dy=0,cancel=False):
    move(*start);command('p')
    for index in range(8):command('m '+str(dx/8)+' '+str(dy/8))
    if cancel:
        snapshots=[state()];key('-k','Escape');snapshots.append(state());command('m 8 0');snapshots.append(state());(out/'cancellation-state.json').write_text(json.dumps(snapshots,indent=2))
    command('r')
def colour_capture(name,coloured):
    rect=json.loads(ipc('geometry','calendar-medium','HEADLESS-1','calendar-day-2026-11-01'))
    def painted():
        call('grim',str(out/name));image=Image.open(out/name).convert('RGB');pixel=image.getpixel((round(rect['x']+rect['width']/2-7),round(rect['y']+(rect['height']-4)/2)))
        return (pixel[0]>pixel[1]+40 and pixel[0]>pixel[2]+40) if coloured else max(pixel)-min(pixel)<30
    wait(painted)

def contrast(style):
    def luminance(value):
        parts=[int(value[index:index+2],16)/255 for index in (1,3,5)]
        parts=[v/12.92 if v<=.04045 else ((v+.055)/1.055)**2.4 for v in parts]
        return sum(a*b for a,b in zip(parts,[.2126,.7152,.0722]))
    light,dark=sorted([luminance(style['foreground']),luminance(style['background'])],reverse=True)
    return (light+.05)/(dark+.05)

try:
    config=out/'sway.conf';config.write_text('output HEADLESS-1 resolution 1280x900 position 0 0\noutput HEADLESS-2 resolution 1000x800 position 1280 0\nxwayland disable\nfor_window [app_id="widget-focus-fixture"] floating enable, resize set 260 160, move position 1000 700\n')
    env['WLR_HEADLESS_OUTPUTS']='2'
    launch([a.sway,'--unsupported-gpu','-c',str(config)],'sway.log');runtime=Path(env['XDG_RUNTIME_DIR']);env['SWAYSOCK']=str(wait(lambda:next(runtime.glob('sway-ipc*.sock'),None)));env['WAYLAND_DISPLAY']=wait(lambda:next((path.name for path in runtime.glob('wayland-*') if not path.name.endswith('.lock')),None))
    launch(['wtype','-s','120000'],'keyboard.log')
    pointer=sp.Popen([a.pointer_tool,'pointer','move','0','0'],env=env,stdin=sp.PIPE,text=True,start_new_session=True);processes.append(pointer);command('m 0.5 0.5')
    launch(['quickshell','-p',str(qml)],'shell.log');wait(lambda:len(state()['rows'])==6 and all(row['loaded'] for row in state()['rows']))
    checks=[];assert not any(row['monochrome'] for row in state()['rows']);assert all(row['opacity']==1 for row in state()['rows'])
    call('swaymsg','focus','output','HEADLESS-1');app=launch(['foot','--app-id=widget-focus-fixture','--title=Widget Focus Fixture','sleep','120'],'app.log');wait(lambda:state()['windowId']>0 and all(row['monochrome'] for row in state()['rows']));assert all(row['opacity']<1 for row in state()['rows']);colour_capture('automatic-active-app.png',False)
    before=len(state()['actions']);click(point('calendar-large','HEADLESS-1','calendar-day-2026-11-02'));wait(lambda:state()['windowId']==0 and not any(row['monochrome'] for row in state()['rows']));assert len(state()['actions'])==before
    call('swaymsg','[app_id="widget-focus-fixture"]','focus');wait(lambda:state()['windowId']>0 and all(row['monochrome'] for row in state()['rows']))
    checks.append('Actual external app focus makes automatic widgets monochrome/dim in painted pixels on both displays; a dimmed calendar remains interactive and restores desktop focus')
    ipc('preference','desktopColor','"color"');wait(lambda:not any(row['monochrome'] for row in state()['rows']));assert state()['windowId']>0 and all(row['opacity']==1 for row in state()['rows']);colour_capture('full-colour-active-app.png',True)
    ipc('preference','desktopColor','"monochrome"');wait(lambda:all(row['monochrome'] for row in state()['rows']));click((1200,500));wait(lambda:state()['windowId']==0);assert all(row['monochrome'] for row in state()['rows']);colour_capture('monochrome-desktop.png',False)
    ipc('preference','desktopColor','"auto"');wait(lambda:not any(row['monochrome'] for row in state()['rows']));colour_capture('automatic-desktop.png',True)
    checks.append('Full colour and monochrome overrides remain explicit while app/desktop focus changes; automatic restores full colour on desktop focus')
    before=len(state()['actions']);click(point('calendar-large','HEADLESS-1','calendar-day-2026-11-02'));wait(lambda:next(row for row in state()['rows'] if row['id']=='calendar-large')['selected']=='2026-11-02');assert len(state()['actions'])==before
    ipc('preference','desktopVisible','false');wait(lambda:all(not row['visible'] for row in state()['rows']));count=state()['backgroundClicks'];click(point('calendar-small','HEADLESS-1'));wait(lambda:state()['backgroundClicks']==count+1);assert len(state()['actions'])==before
    ipc('preference','desktopVisible','true');wait(lambda:all(row['visible'] for row in state()['rows']));assert next(row for row in state()['rows'] if row['id']=='calendar-large')['selected']=='2026-11-02'
    checks.append('Hide releases widget input mask to background; showing preserves exact widget identity and manual calendar selection')
    ipc('update','calendar-small','{"output":"HEADLESS-2"}');wait(lambda:any(row['id']=='calendar-small' and row['output']=='HEADLESS-2' for row in state()['rows']));assert not any(row['id']=='calendar-small' and row['output']=='HEADLESS-1' for row in state()['rows']);ipc('update','calendar-small','{"output":"HEADLESS-1"}');wait(lambda:any(row['id']=='calendar-small' and row['output']=='HEADLESS-1' for row in state()['rows']))
    checks.append('Per-display preference routes its selected widget to the chosen display and removes the old target')
    ipc('preference','desktopColor','"monochrome"');ipc('preference','reduceTransparency','true');wait(lambda:all(row['opaque'] and row['alpha']==1 and row['opacity']==1 for row in state()['rows']));call('grim',str(out/'opaque-monochrome.png'))
    image=Image.open(out/'opaque-monochrome.png').convert('RGB');clock=next(row for row in state()['rows'] if row['id']=='clock-large');sample=image.getpixel((round(clock['screenX']+clock['x']+130),round(clock['screenY']+clock['y']+15)));assert max(abs(sample[i]-[40,40,43][i]) for i in range(3))<=2,sample
    checks.append('Reduce Transparency makes calendar and clock outer materials fully opaque without opacity-based dimming, confirmed by compositor pixels')
    ipc('preference','reduceTransparency','false');appearance.write_text(json.dumps(dict(font=palettes['font'],palette=palettes['dark'],preferences=dict(textScale=1.5,reduceTransparency=True))));wait(lambda:all(row['opaque'] and row['alpha']==1 and row['opacity']==1 for row in state()['rows']))
    before=len(state()['actions'])
    for size in ('small','medium','large'):
        identifier='calendar-'+size;click(point(identifier,'HEADLESS-1','calendar-grid' if size!='small' else 'widget-calendar-content',.1,.1));ipc('focusRelated',identifier,'HEADLESS-1');time.sleep(.2);row=next(row for row in state()['rows'] if row['id']==identifier);assert row['related']['y']>=row['y'] and row['related']['y']+row['related']['height']<=row['y']+row['height'],row;key('-k','space');assert len(state()['actions'])==before+1;before+=1
        identifier='clock-'+size;click(point(identifier,'HEADLESS-2','widget-clock-content'));ipc('focusClock',identifier,'HEADLESS-2');key('-k','space');assert len(state()['actions'])==before+2;before+=2
    call('grim',str(out/'enlarged-opaque.png'));style=json.loads(ipc('dayStyle'));assert style['font']>=18 and contrast(style)>=4.5,style
    appearance.write_text(json.dumps(dict(font=palettes['font'],palette=palettes['light'],preferences=dict(textScale=1.5,reduceTransparency=True))));wait(lambda:json.loads(ipc('dayStyle'))['foreground']=='#ffffff');style=json.loads(ipc('dayStyle'));assert contrast(style)>=4.5,style;colour_capture('enlarged-light-monochrome.png',False)
    checks.append('At 150 percent text and global Reduce Transparency, all three supported calendar/clock sizes retain reachable distinct pointer/keyboard actions; selected-day contrast passes in both live palettes')
    log=(out/'shell.log').read_text();assert not any(marker in log for marker in ('WARN scene:','ReferenceError:','TypeError:','Binding loop','Failed to load configuration','Cannot assign')),log
    assert not state()['error'],state()
    (out/'result.json').write_text(json.dumps(dict(passed=True,checks=checks,final=state(),scope='Two private native outputs, actual production focus bridge/external app, synthetic read-only calendars and disposable widget settings'),indent=2)+'\n')
finally:
    for process in reversed(processes):
        if process.poll() is None:os.killpg(process.pid,signal.SIGTERM)
    if 'pointer' in globals() and pointer.stdin:pointer.stdin.close()
    for process in reversed(processes):
        try:process.wait(timeout=4)
        except sp.TimeoutExpired:os.killpg(process.pid,signal.SIGKILL)
print('PASS native widget focus, colour, visibility, opaque material and enlarged controls')

"""Calendar widget read-only data and native activation acceptance."""
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
env.update(WLR_BACKENDS='headless',WLR_RENDERER='gles2' if a.effects else 'pixman',WLR_RENDERER_FORCE_SOFTWARE='1' if a.effects else '0',WLR_RENDERER_ALLOW_SOFTWARE='1',LIBGL_ALWAYS_SOFTWARE='1',WLR_LIBINPUT_NO_DEVICES='1',QT_QPA_PLATFORM='wayland',QT_QPA_PLATFORMTHEME='generic',QT_QUICK_BACKEND='software',QT_ACCESSIBILITY='0',QT_LINUX_ACCESSIBILITY_ALWAYS_ON='0',NO_AT_BRIDGE='1',GTK_A11Y='none',LUMEN_FILE_SERVICE='0',LUMEN_LAUNCH_SCOPES='0',LUMEN_NATIVE_WINDOW_CONTROLS='0',LUMEN_NATIVE_WINDOW_ACTIONS='0',LUMEN_UNIFIED_OWNED_HEADERS='1')
for key in ('DISPLAY','WAYLAND_DISPLAY','SWAYSOCK','AT_SPI_BUS_ADDRESS'):env.pop(key,None)
palettes=json.loads((repo/'res/lumen-desktop/appearance/palette.json').read_text());appearance=out/'appearance.json';appearance.write_text(json.dumps(dict(font=palettes['font'],palette=palettes['dark'],preferences={})));env['LUMEN_APPEARANCE_STATE']=str(appearance);env['GLOBAL_MENU_PYTHON']=sys.executable
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
qml=out/'shell.qml';qml.write_text('''import QtQuick
import QtQuick.Window
import Quickshell
import Quickshell.Io
import "assets/shell" as D
ShellRoot {
 id:root
 property date currentDate:new Date(2026,9,31,12)
 property var actions:[]
 property var widgets:widgetService
 QtObject {id:widgetService;property var preferences:({reduceTransparency:false});function positionWidget(id,output,x,y){}function edit(){}}
 function openCalendarAt(day,event){actions=actions.concat([{kind:event?"event":"related",day:Qt.formatDate(day,"yyyy-MM-dd"),id:event?event.id:null,title:event?event.title:null,provider:event?event.provider:null,calendar:event?event.calendar:null}]);}
 function openSettings(page){actions=actions.concat([{kind:"settings",page:page}]);}
 function showMenu(name,output){actions=actions.concat([{kind:"menu",name:name}]);}
 FloatingWindow {id:window;title:"Calendar widget fixture";visible:true;implicitWidth:500;implicitHeight:760;color:D.Theme.colors.background
  D.DesktopWidgetItem {id:widget;shell:root;descriptor:({id:"fixture-calendar",type:"calendar",size:"large",style:"month",positions:({}),output:"*"});outputName:"HEADLESS-1";widgetIndex:0;availableWidth:window.width;availableHeight:window.height;active:true}
 }
 function find(name,item){if(!item)return null;if(item.objectName===name)return item;for(const child of item.children || []){const result=find(name,child);if(result)return result;}return null;}
 function card(){return find("widget-calendar-content",widget);}
 IpcHandler {target:"filesAudit"
  function eventName():string {return find("calendar-event-"+card().events[1].id,widget).Accessible.name;}
  function wallTime():string {return card().timeLabel({date:"2026-03-29",startTime:"01:30",allDay:false});}
  function refresh():void {card().refresh();}
  function clock(value:string):void {root.currentDate=new Date(value+"T12:00:00");}
  function select(value:string):void {card().selectDate(new Date(value+"T12:00:00"));}
  function today():void {card().goToday();}
  function pause(value:bool):void {card().active=!value;}
  function focusGrid():void {find("calendar-grid",widget).forceActiveFocus();}
  function focusRelated():void {find("calendar-open-related",widget).forceActiveFocus();}
  function focusEvent():void {find("calendar-event-"+card().events[1].id,widget).forceActiveFocus();}
  function state():string {const c=card();const related=find("calendar-open-related",widget);const at=related.mapToItem(window.contentItem,0,0);return JSON.stringify({ready:!c.busy && !c.refreshing,selected:c.iso(c.selected),month:c.iso(c.month),following:c.followingToday,events:c.events,loaded:c.loaded,error:c.error,actions:root.actions,related:{y:at.y,height:related.height},focus:window.contentItem.Window.window.activeFocusItem?window.contentItem.Window.window.activeFocusItem.objectName:""});}
  function geometry(name:string):string {const item=find(name,widget);if(!item)return "null";let ancestor=item.parent;while(ancestor){if(ancestor.contentY!==undefined && ancestor.contentItem){const pos=item.mapToItem(ancestor.contentItem,0,0);ancestor.contentY=Math.max(0,Math.min(ancestor.contentHeight-ancestor.height,pos.y-80));break;}ancestor=ancestor.parent;}const at=item.mapToItem(window.contentItem,0,0);return JSON.stringify({x:at.x,y:at.y,width:item.width,height:item.height});}
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
def point(name,rx=.5,ry=.5):
    value=json.loads(ipc('geometry',name));assert value,name;r=window()['rect'];return (r['x']+value['x']+value['width']*rx,r['y']+value['y']+min(value['height'],25)*ry)
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
try:
    config=out/'sway.conf';config.write_text('output HEADLESS-1 resolution 1280x900\nxwayland disable\nfor_window [all] floating enable, border pixel 0\n')
    launch([a.sway,'--unsupported-gpu','-c',str(config)],'sway.log');runtime=Path(env['XDG_RUNTIME_DIR']);env['SWAYSOCK']=str(wait(lambda:next(runtime.glob('sway-ipc*.sock'),None)));env['WAYLAND_DISPLAY']=wait(lambda:next((path.name for path in runtime.glob('wayland-*') if not path.name.endswith('.lock')),None))
    launch(['wtype','-s','120000'],'keyboard.log')
    pointer=sp.Popen([a.pointer_tool,'pointer','move','0','0'],env=env,stdin=sp.PIPE,text=True,start_new_session=True);processes.append(pointer);command('m 0.5 0.5')
    launch(['quickshell','-p',str(qml)],'shell.log');wait(lambda:window());wait(lambda:state()['ready'] and state()['loaded'])
    checks=[];assert '1:30' in ipc('wallTime'),ipc('wallTime');checks.append('Calendar-zone wall-time label is not normalised through the host DST gap');initial=state();assert initial['selected']=='2026-10-31' and len(initial['events'])==2,initial
    (out/'delay').touch();ipc('clock','2026-11-01');immediate=state();assert immediate['selected']=='2026-11-01' and immediate['month']=='2026-11-01' and not immediate['events'],immediate
    wait(lambda:state()['ready'] and len(state()['events'])==4);(out/'delay').unlink();mixed=state();assert [e['startTime'] for e in mixed['events']]==['00:00','01:30','02:00','03:00'];assert mixed['events'][0]['allDay'] and mixed['events'][1]['calendar']=='Travel'
    checks.append('Following-today month rollover clears stale prior-day rows while delayed read-only provider returns mixed all-day, overlapping and cross-timezone data')
    before=len(state()['actions']);click(point('widget-calendar-content',.25,.4));assert len(state()['actions'])==before and state()['selected']=='2026-11-01';click(point('calendar-day-2026-11-02'));wait(lambda:state()['selected']=='2026-11-02' and state()['ready']);assert state()['events']==[] and len(state()['actions'])==before
    ipc('clock','2026-12-01');assert state()['selected']=='2026-11-02' and state()['month']=='2026-11-01' and not state()['following']
    checks.append('Actual day pointer activation selects once without opening the app; explicit selection survives later clock/month rollover')
    ipc('clock','2026-11-01');ipc('today');wait(lambda:state()['ready'] and len(state()['events'])==4)
    wanted=state()['events'][1];assert 'iCloud' in ipc('eventName') and 'Travel' in ipc('eventName') and 'Europe/London' in ipc('eventName');click(point('calendar-event-'+wanted['id']));assert len(state()['actions'])==before+1 and state()['actions'][-1]['id']==wanted['id'] and state()['actions'][-1]['day']=='2026-11-01'
    ipc('focusGrid')
    for _ in range(12):
        key('-k','Tab')
        if state()['focus']=='calendar-event-'+wanted['id']:break
    assert state()['focus']=='calendar-event-'+wanted['id'],state()
    key('-k','space');assert len(state()['actions'])==before+2 and state()['actions'][-1]['id']==wanted['id']
    checks.append('Actual event pointer and keyboard activations each open exactly the same provider/calendar/event identity once')
    for _ in range(12):
        key('-k','Tab')
        if state()['focus']=='calendar-open-related':break
    assert state()['focus']=='calendar-open-related',state()
    time.sleep(.2);assert state()['related']['y']>=0 and state()['related']['y']+state()['related']['height']<760,state();key('-k','space');assert len(state()['actions'])==before+3 and state()['actions'][-1]['kind']=='related' and state()['actions'][-1]['day']=='2026-11-01'
    click(point('calendar-open-related'));assert len(state()['actions'])==before+4 and state()['actions'][-1]['kind']=='related' and state()['actions'][-1]['day']=='2026-11-01'
    checks.append('Related-app link is distinct, reachable after focus scrolling, and each pointer/keyboard activation opens exactly once')
    call('grim',str(out/'mixed-widget-dark.png'))
    ipc('focusGrid');key('-k','Right');assert state()['selected']=='2026-11-02' and state()['events']==[] and len(state()['actions'])==before+4
    checks.append('Actual calendar-grid arrow navigation changes the date once without invoking the associated app')
    ipc('clock','2026-04-30');ipc('today');wait(lambda:state()['ready']);ipc('pause','true');position=point('calendar-day-2026-04-10');move(*position);command('p');ipc('clock','2026-05-01');command('r');wait(lambda:state()['selected']=='2026-04-10' and state()['ready'])
    checks.append('Month rollover during a held day press preserves the pressed date identity instead of activating the replacement month cell')
    ipc('clock','2026-04-10');ipc('today');wait(lambda:state()['ready']);position=point('calendar-open-related');move(*position);command('p');count=len(state()['actions']);ipc('clock','2026-04-11');command('r');assert len(state()['actions'])==count+1 and state()['actions'][-1]['day']=='2026-04-10' and state()['selected']=='2026-04-11',state()
    checks.append('Related-app press captures its date across a clock tick rather than opening a different release-time date')
    ipc('pause','false');wait(lambda:state()['ready'])
    (out/'unavailable').touch();ipc('refresh');wait(lambda:state()['ready'] and 'unavailable' in state()['error']);assert state()['selected']=='2026-04-11';(out/'unavailable').unlink()
    checks.append('Provider failure is explicit and cannot silently select a different date or invoke an app')
    log=(out/'shell.log').read_text();assert not any(marker in log for marker in ('WARN scene:','ReferenceError:','TypeError:','Binding loop','Failed to load configuration','Cannot assign')),log
    requests=[json.loads(line) for line in (out/'requests.jsonl').read_text().splitlines()];assert all(row['action'] in ('list','load') for row in requests)
    (out/'result.json').write_text(json.dumps(dict(passed=True,checks=checks,final=state(),requests=requests,scope='Private compositor, actual desktop calendar widget and pointer/keyboard; synthetic local ICS mirror only'),indent=2)+'\n')
finally:
    for process in reversed(processes):
        if process.poll() is None:os.killpg(process.pid,signal.SIGTERM)
    if 'pointer' in globals() and pointer.stdin:pointer.stdin.close()
    for process in reversed(processes):
        try:process.wait(timeout=4)
        except sp.TimeoutExpired:os.killpg(process.pid,signal.SIGKILL)
print('PASS native calendar widget mixed data, rollover and exact single-action identity')

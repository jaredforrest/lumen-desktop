"""Isolated Calendar/Notification Centre regression: never uses personal accounts.

Run with tests/desktop/runtime.nix's desktop-test-python. The real QML components
use a fixture calendar executable and in-memory notification transport. No save
or remote mutation is permitted by that executable.
"""
import argparse,json,os,re,shlex,signal,subprocess as sp,sys,tempfile,time
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--output',type=Path,required=True);p.add_argument('--inside',action='store_true');a=p.parse_args()
if not a.inside:raise SystemExit(sp.call(['dbus-run-session','--',sys.executable,__file__,*sys.argv[1:],'--inside']))
import shutil
repo=Path(__file__).resolve().parents[2];assets=repo/'res/lumen-desktop/shell';run=a.output.resolve();run.mkdir(parents=True,exist_ok=False)
launcher=Path(shutil.which('lumen-desktop-shell'));prefix=re.split(r'\n[ \t]*exec quickshell ',launcher.read_text(),maxsplit=1)[0]
raw=sp.check_output(['bash','-c',prefix+'\nexec env -0'],stderr=sp.DEVNULL);installed=dict(x.decode().split('=',1) for x in raw.split(b'\0') if b'=' in x)
env=dict(os.environ)
for k in ('PATH','QT_PLUGIN_PATH','QML_IMPORT_PATH','QT_QPA_PLATFORMTHEME'):
 if k in installed:env[k]=installed[k]
runtime=Path(tempfile.mkdtemp(prefix='calendar-test-'));env.update(HOME=str(run/'home'),XDG_RUNTIME_DIR=str(runtime),XDG_CONFIG_HOME=str(run/'config'),XDG_DATA_HOME=str(run/'data'),XDG_STATE_HOME=str(run/'state'),XDG_CACHE_HOME=str(run/'cache'),WLR_BACKENDS='headless',WLR_RENDERER='pixman',WLR_LIBINPUT_NO_DEVICES='1',QT_QPA_PLATFORM='wayland',GDK_BACKEND='wayland',LUMEN_COMPOSITOR_EFFECTS='0',GLOBAL_MENU_ENABLED='0',LUMEN_FILE_SERVICE='0',LUMEN_LAUNCH_SCOPES='0')
(run/'state/lumen-desktop/session').mkdir(parents=True);(run/'home').mkdir();fake=run/'bin';fake.mkdir()
calendar=fake/'lumen-calendar';calendar.write_text('#!'+sys.executable+'''\nimport json,sys
r=json.loads(sys.stdin.readline());assert r['action'] in ('list','load','sync'), 'Mutations forbidden in UI regression'
month=r.get('month','2026-10-01');day=r.get('selectedDate','2026-10-14')
events=[dict(id=str(i),title='A long project review title that must remain readable' if i==0 else 'Example appointment '+str(i),startTime='09:00',endTime='10:00',allDay=False,provider='Google',calendar='Work' if i%2 else 'Personal',color='#aa44cc' if i%2 else '#008855',date=day,endDate=day,notes='Synthetic fixture',location='Test room',editable=True,editStart=day+'T09:00',editEnd=day+'T10:00',accountGeneration='fixture',calendarId='work',eventId=str(i)) for i in range(8)]
print(json.dumps(dict(configured=True,timezone='Europe/London',lastSuccess='2026-10-14T09:00:00Z',month=month,date=day,events=events,monthEvents=events,days=[day],total=8,providers=[dict(id='google',name='Google',configured=True,generation='fixture',calendars=[dict(id='work',name='Work',accessRole='writer')])])) )
''');calendar.chmod(0o700);env['PATH']=str(fake)+':'+env['PATH']
for k in ('DISPLAY','WAYLAND_DISPLAY','SWAYSOCK'):env.pop(k,None)
qml=run/'shell.qml';qml.write_text('''import QtQuick
import Quickshell
import Quickshell.Wayland
import Quickshell.Io
import "file:'''+str(assets)+'''" as Desktop
import "file:'''+str(assets/'CalendarLogic.js')+'''" as Logic
ShellRoot {
 id:root
 property date now:new Date(2026,9,14,12,0)
 QtObject {id:notifications
  property bool available:true
  property bool dnd:false
  property string error:""
  property var expandedCards:({})
  property var commands:[]
  property var policies:({"other-app":"banners"})
  property var history:Array.from({length:3},(_,i)=>({id:i+1,app:"Fixture App",desktop:"fixture-app",summary:"Example notification "+i,body:"A synthetic notification. No personal history is read.",timestamp:Date.now(),actions:[{id:"default",label:"Open"}],icon:"",image:""}))
  property var groups:[{key:"fixture-app",name:"Fixture App",rows:history}]
  function command(data){commands=commands.concat([data]);if(data.action==="policy"){const next=Object.assign({},policies);next[data.app]=data.policy;policies=next;}}
  function invoke(row,key){command({action:"invoke",id:row.id,key:key});}
  function seen(){}
  function toggleExpanded(id){const next=Object.assign({},expandedCards);next[id]=!next[id];expandedCards=next;}
 }
 QtObject {id:stub
  property var notifications:notifications
  property date currentDate:root.now
  function openCalendarAt(day,event){full.openAt(day,event);}
  function openSettings(page){}
  function claimPopup(name){}
  function focusOwnedWindow(name){}
 }
 Desktop.CalendarWindow {id:full;shell:stub}
 PanelWindow {visible:!full.visible;WlrLayershell.keyboardFocus:WlrKeyboardFocus.Exclusive;anchors {top:true;bottom:true;left:true;right:true} color:"#798a9c"
  Flickable {id:viewport;x:12;y:12;width:Math.min(344,parent.width-24);height:parent.height-24;contentWidth:width;contentHeight:panel.height;clip:true
   Desktop.NotificationPanel {id:panel;width:viewport.width;shell:stub;today:root.now}
  }
 }
 function find(item,name){if(item.objectName===name)return item;for(const child of item.children || []){const result=find(child,name);if(result)return result;}return null;}
 function card(){return find(panel,"calendar-widget");}
 IpcHandler {target:"audit"
  function status():string{return JSON.stringify(Object.assign(panel.status(),{scale:viewport.scale,viewportHeight:viewport.height,contentHeight:viewport.contentHeight,commands:notifications.commands,policies:notifications.policies,editorVisible:full.visible,gridFocus:find(card(),"calendar-grid").activeFocus}));}
  function check():string {
   if(Logic.iso(Logic.monthShift(new Date(2024,0,31),1))!=="2024-02-29")throw Error("Leap-year month navigation");
   if(Logic.iso(Logic.monthShift(new Date(2026,0,31),1))!=="2026-02-28")throw Error("Month-end clamp");
   if(Logic.offset(new Date(2026,10,1),1)!==6 || Logic.offset(new Date(2026,10,1),7)!==0)throw Error("Locale first weekday");
   if(Logic.tint({color:"#aa44cc"},false)!=="#aa44cc")throw Error("Provider color lost");
   for(const name of ["en_US","en_GB"]){const locale=Qt.locale(name),labels=Logic.weekdayNames(locale,locale.firstDayOfWeek,Locale.ShortFormat);
    if(labels.length!==7 || labels.some(x=>!x) || labels[0]!==locale.standaloneDayName(locale.firstDayOfWeek,Locale.ShortFormat) || labels[6]!==locale.standaloneDayName((locale.firstDayOfWeek+6)%7,Locale.ShortFormat))throw Error("Weekday header shifted for "+name);
   }
   if(Logic.clockDatePattern(Qt.locale("en_US"),Locale.LongFormat)!=="ddd MMM d" || Logic.clockDatePattern(Qt.locale("en_GB"),Locale.LongFormat)!=="ddd d MMM")throw Error("Clock month/day order does not follow locale");
   const c=card();if(c.dayColors(c.selected).length!==2)throw Error("Month-grid calendar colours lost");c.selectDate(new Date(root.now));root.now=new Date(2026,9,15,12,0);
   if(c.iso(c.selected)!=="2026-10-15")throw Error("Today stopped following midnight");
   c.selectDate(new Date(2026,8,1));root.now=new Date(2026,9,16,12,0);
   if(c.iso(c.selected)!=="2026-09-01")throw Error("Midnight stole manual selection");
   c.selectDate(new Date(2026,0,31));c.moveDate(Qt.Key_PageDown);
   if(c.iso(c.selected)!=="2026-02-28")throw Error("Keyboard month navigation");
   c.goToday();const selectedCell=find(c,"calendar-day-"+c.iso(c.selected));
   if(!selectedCell || !selectedCell.Accessible.checked || !selectedCell.Accessible.name.includes("today"))throw Error("Accessible selected/today state missing");
   const grid=find(c,"calendar-grid");grid.forceActiveFocus();
   return "PASS date rollover, manual browsing, leap years, locale offsets, month navigation and provider colors";
  }
  function focusGrid():void {find(card(),"calendar-grid").forceActiveFocus();}
  function preferences(style:string,first:bool,show:bool):void {panel.setPreference("style",style);panel.setPreference("calendarFirst",first);panel.setPreference("calendarVisible",show);}
  function today():void {card().goToday();}
  function collapse():void {const top=find(panel,"notification-card");if(top && !top.expandStack)panel.children.filter(x=>x.expandedGroups!==undefined).forEach(x=>x.expandedGroups=({}));}
  function editWidgets():void {panel.editingWidgets=true;}
  function editor():void {full.open();full.create(new Date(2026,9,14));}
  function editorCheck():string {
   const editor=full.calendarState;const field=find(editor,"date-time-starts");
   if(!field)throw Error("Date/time control missing");
   find(field,"date-time-starts-year").value=2028;find(field,"date-time-starts-month").currentIndex=1;find(field,"date-time-starts-day").value=29;
   find(field,"date-time-starts-hour").value=23;find(field,"date-time-starts-minute").value=45;field.commit();
   if(field.text!=="2028-02-29T23:45")throw Error("Date/time controls did not serialize leap-day time: "+field.text);
   field.allDay=true;field.commit();if(field.text!=="2028-02-29")throw Error("All-day serialization");
   return "PASS accessible states and date/time controls preserve leap dates, time and all-day values";
  }
  function notificationPolicy():string {
   const top=find(panel,"notification-card");top.deliveryMenu.itemAt(1).triggered();
   if(notifications.policies["fixture-app"]!=="silent" || notifications.policies["other-app"]!=="banners")throw Error("Per-app policy changed the wrong application");return "PASS per-app notification preference isolation";
  }
  function notificationCheck():string {
   const top=find(panel,"notification-card");const dismiss=find(top,"notification-dismiss");if(dismiss.visible)throw Error("Dismiss control should stay hidden at rest");top.forceActiveFocus();if(!dismiss.visible)throw Error("Keyboard focus must reveal dismissal");if(!top || !top.expandStack)throw Error("Missing collapsed stack");
   top.activate();if(notifications.commands.some(x=>x.action==="invoke"))throw Error("Stack expansion opened app");
   return "PASS stack expansion is distinct from opening notification";
  }
  function notificationOpen():string {const top=find(panel,"notification-card");if(top.expandStack)throw Error("Stack remained collapsed");top.activate();if(!notifications.commands.some(x=>x.action==="invoke" && x.id===1))throw Error("Expanded alert did not open");return "PASS expanded notification opens selected item";}
 }
}''')
env.update(QT_ACCESSIBILITY='0',QT_LINUX_ACCESSIBILITY_ALWAYS_ON='0',NO_AT_BRIDGE='1',GTK_A11Y='none')
env.pop('AT_SPI_BUS_ADDRESS',None)
procs=[]
def launch(cmd,name):
 with (run/(name+'.log')).open('w') as _fixture_log0:
     pr=sp.Popen(cmd,env=env,stdout=_fixture_log0,stderr=sp.STDOUT,start_new_session=True);procs.append(pr);return pr
def call(*cmd):return sp.check_output(cmd,env=env,text=True,timeout=15)
def wait(fn):
 end=time.monotonic()+20
 while time.monotonic()<end:
  try:
   if fn():return
  except (ValueError,OSError,sp.SubprocessError):pass
  time.sleep(.12)
 raise AssertionError('Readiness/transaction timed out; inspect fixture logs')
def ipc(method,*v):return call('quickshell','-p',str(qml),'ipc','call','audit',method,*v)
def status():return json.loads(ipc('status'))
try:
 conf=run/'sway.conf';conf.write_text('output HEADLESS-1 resolution 900x1000\nxwayland disable\nfor_window [app_id="org.quickshell"] floating enable, border none\n')
 launch(['sway','--unsupported-gpu','-c',str(conf)],'sway');wait(lambda:list(runtime.glob('sway-ipc*.sock')))
 env['SWAYSOCK']=str(next(runtime.glob('sway-ipc*.sock')));env['WAYLAND_DISPLAY']=next(x.name for x in runtime.glob('wayland-*') if not x.name.endswith('.lock'))
 palettes=json.loads((assets.parent/'appearance/palette.json').read_text());checks=[]
 for theme in ('light','dark'):
  call('swaymsg','output HEADLESS-1 resolution 900x1000 scale 1');time.sleep(.2)
  appearance=run/(theme+'.json');appearance.write_text(json.dumps({'font':palettes['font'],'palette':palettes[theme],'preferences':{'mode':theme,'reduceMotion':True}}));env['LUMEN_APPEARANCE_STATE']=str(appearance)
  shell=launch(['quickshell','-p',str(qml)],theme);wait(lambda:status()['events']==8 and not status()['busy'])
  print(ipc('check').strip(),flush=True);wait(lambda:not status()['busy'])
  keyboard=sp.Popen(['wtype','-s','500','-k','Right'],env=env);time.sleep(.2);ipc('focusGrid');keyboard.wait(timeout=5);wait(lambda:status()['selected']=='2026-10-17');checks.append(theme+' real keyboard date navigation')
  ipc('today');wait(lambda:not status()['busy']);call('grim',str(run/(theme+'-collapsed.png')))
  print(ipc('notificationCheck').strip(),flush=True);time.sleep(.2);print(ipc('notificationOpen').strip(),flush=True);print(ipc('notificationPolicy').strip(),flush=True)
  call('grim',str(run/(theme+'-panel.png')))
  for resolution,scale in (('1920x1080','1'),('1920x1080','2'),('1280x720','2')):
   call('swaymsg','output HEADLESS-1 resolution '+resolution+' scale '+scale);time.sleep(.25)
   state=status();assert state['scale']==1 and state['contentHeight']>0 and state['width']<=344,state
   call('grim',str(run/(theme+'-'+resolution+'-'+scale+'.png')))
   checks.append(theme+' readable panel at '+resolution+' scale '+scale)
  ipc('preferences','agenda','true','false');wait(lambda:not status()['calendarVisible'] and status()['style']=='agenda' and status()['calendarFirst'])
  preference_file=run/'state/lumen-desktop/session/notification-centre.json';wait(lambda:preference_file.exists() and json.loads(preference_file.read_text())['calendarVisible'] is False)
  call('swaymsg','output HEADLESS-1 resolution 900x1000 scale 1');time.sleep(.2)
  ipc('preferences','month','false','true');wait(lambda:status()['calendarVisible']);ipc('editWidgets');call('grim',str(run/(theme+'-widget-editor.png')))
  ipc('editor');wait(lambda:status()['editorVisible']);time.sleep(.5);print(ipc('editorCheck').strip(),flush=True);call('grim',str(run/(theme+'-event-editor.png')))
  os.killpg(shell.pid,signal.SIGTERM);shell.wait(timeout=5)
  # A real restart must load the saved preferences rather than only in-memory state.
  shell=launch(['quickshell','-p',str(qml)],theme+'-restart');wait(lambda:status()['style']=='month' and status()['calendarVisible'] and not status()['calendarFirst']);checks.append(theme+' saved preferences survive restart')
  os.killpg(shell.pid,signal.SIGTERM);shell.wait(timeout=5)
  for suffix in ('', '-restart'):
   log=(run/(theme+suffix+'.log')).read_text();assert not any(x in log for x in ('Failed to load configuration','ReferenceError:','TypeError:','Binding loop')),log
 (run/'results.json').write_text(json.dumps({'passed':True,'checks':checks},indent=2));print('PASS calendar/notification regression and capture matrix',flush=True)
finally:
 for pr in reversed(procs):
  if pr.poll() is None:
   os.killpg(pr.pid,signal.SIGTERM)
   try:pr.wait(timeout=5)
   except sp.TimeoutExpired:os.killpg(pr.pid,signal.SIGKILL)
 shutil.rmtree(runtime,ignore_errors=True)

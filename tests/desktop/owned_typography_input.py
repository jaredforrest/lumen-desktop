"""Physical essential controls at enlarged text in private production windows."""
import argparse,hashlib,json,os,re,shutil,signal,subprocess as sp,sys,time
from pathlib import Path
from application_fixture import isolation
from application_fixture.keyboard import PhysicalKeyboard
from private_native_bus import start_private_bus
from fixture_resources import close_reaped_pipes
p=argparse.ArgumentParser();p.add_argument('--launcher',type=Path,required=True);p.add_argument('--sway',type=Path,required=True);p.add_argument('--keyboard',type=Path,required=True);p.add_argument('--font-config',type=Path,required=True);p.add_argument('--software-driver',type=Path,required=True);p.add_argument('--font-family');p.add_argument('--expected-resolved-family');p.add_argument('--effects',action='store_true');p.add_argument('--inside',action='store_true');p.add_argument('--output',type=Path,required=True);a=p.parse_args();repo=Path(__file__).resolve().parents[2]
for name in ('launcher','sway','keyboard','font_config','software_driver'):
 value=getattr(a,name).resolve();assert str(value).startswith('/nix/store/') and value.exists(),name;setattr(a,name,value)
if not a.inside:
 assert not sp.check_output(['git','-C',str(repo),'status','--porcelain'],text=True).strip(),'Freeze source before acceptance'
 revision=sp.check_output(['git','-C',str(repo),'rev-parse','HEAD'],text=True).strip();out=a.output.resolve();out.mkdir(mode=0o700,parents=True,exist_ok=False)
 prefix=re.split(r'\n[ \t]*exec quickshell ',a.launcher.read_text(),maxsplit=1)[0];env=dict(value.decode().split('=',1)for value in sp.check_output(['bash','-c',prefix+'\nexec env -0']).split(b'\0')if b'='in value);env['FONTCONFIG_FILE']=str(a.font_config)
 command=[sys.executable,'/source/tests/desktop/owned_typography_input.py','--inside','--launcher',str(a.launcher),'--sway',str(a.sway),'--keyboard',str(a.keyboard),'--font-config',str(a.font_config),'--software-driver',str(a.software_driver),'--output','/artifacts']+(['--effects']if a.effects else [])+(['--font-family',a.font_family]if a.font_family else [])+(['--expected-resolved-family',a.expected_resolved_family]if a.expected_resolved_family else [])
 source_hash=hashlib.sha256(Path(__file__).read_bytes()).hexdigest();code=isolation.run(repo,out,command,env,software_graphics=a.effects);unchanged=sp.check_output(['git','-C',str(repo),'rev-parse','HEAD'],text=True).strip()==revision and not sp.check_output(['git','-C',str(repo),'status','--porcelain'],text=True).strip() and hashlib.sha256(Path(__file__).read_bytes()).hexdigest()==source_hash;(out/'provenance.json').write_text(json.dumps(dict(revision=revision,exitCode=code,launcher=str(a.launcher),compositor=str(a.sway),fontConfig=str(a.font_config),fontConfigSha256=hashlib.sha256(a.font_config.read_bytes()).hexdigest(),sourceSha256=source_hash,sourceUnchanged=unchanged),indent=2));assert unchanged,'Fixture source changed during acceptance';raise SystemExit(code)
assert repo==Path('/source') and os.environ['HOME']=='/home/fixture' and os.getuid()!=0 and not Path('/.old-root').exists()
assert not any((Path('/dev')/name).exists()for name in ('dri','snd','video0','tty'));out=a.output;env=dict(os.environ,LANG='C.UTF-8',LC_ALL='C.UTF-8',WLR_BACKENDS='headless',WLR_HEADLESS_OUTPUTS='1',WLR_RENDERER='gles2'if a.effects else 'pixman',WLR_RENDERER_FORCE_SOFTWARE='1'if a.effects else '0',WLR_RENDERER_ALLOW_SOFTWARE='1',WLR_LIBINPUT_NO_DEVICES='1',LIBGL_ALWAYS_SOFTWARE='1',QT_QPA_PLATFORM='wayland',QT_QUICK_BACKEND='software',QT_QPA_PLATFORMTHEME='generic',QT_ACCESSIBILITY='0',NO_AT_BRIDGE='1',GTK_A11Y='none',LUMEN_UNIFIED_OWNED_HEADERS='1',LUMEN_NATIVE_WINDOW_CONTROLS='0',LUMEN_NATIVE_WINDOW_ACTIONS='0',LUMEN_FILE_SERVICE='0',LUMEN_LAUNCH_SCOPES='0',LUMEN_NEARBY_SHARING_LABEL='Nearby Sharing',LUMEN_NEARBY_SHARING_PROVIDER='',GLOBAL_MENU_PYTHON=sys.executable)
env['FONTCONFIG_FILE']=str(a.font_config);env['MESA_LOADER_DRIVER_OVERRIDE']='llvmpipe';env['LIBGL_DRIVERS_PATH']=str(a.software_driver/'lib/dri');env['__EGL_VENDOR_LIBRARY_FILENAMES']=str(a.software_driver/'share/glvnd/egl_vendor.d/50_mesa.json')
assets=out/'desktop';shutil.copytree(repo/'res/lumen-desktop/shell',assets);backend=assets/'files.py';original=backend.read_text();needle="    if action=='volumes':\n";assert original.count(needle)==1;backend.write_text(original.replace(needle,needle+'        return dict(volumes=[])\n'));(out/'provider-seam.json').write_text(json.dumps(dict(before=hashlib.sha256(original.encode()).hexdigest(),after=hashlib.sha256(backend.read_bytes()).hexdigest())))
folder=Path('/home/fixture/Documents');folder.mkdir();(folder/'An ordinary document.txt').write_text('Private fixture unchanged\n');(folder/'Another document.txt').write_text('Second private document\n');palette=json.loads((repo/'res/lumen-desktop/appearance/palette.json').read_text());palette['font']=a.font_family or palette['font'];appearance=out/'appearance.json';env['LUMEN_APPEARANCE_STATE']=str(appearance)
def theme(mode):
 pending=appearance.with_suffix('.pending');pending.write_text(json.dumps(dict(font=palette['font'],palette=palette[mode],preferences=dict(textScale=1.5,reduceMotion=True,reduceTransparency=True))));pending.replace(appearance)
theme('light');qml=out/'fixture.qml';qml.write_text('''import QtQuick
import QtQuick.Controls
import QtQuick.Window
import Quickshell
import Quickshell.Io
import "desktop" as D
ShellRoot {
 id:audit
 property string cancellationResult:""
 QtObject {id:exported;property var state:({appName:"",title:"",windowId:0})}
 QtObject {id:stub;property date currentDate:new Date(2026,9,20);property int focusEpoch:0;property var windows:[];property var exportedMenu:exported;function activeFiles(window){}function focusWindow(window){}function claimPopup(name){}function focusOwnedWindow(name){}function openSettings(name){}}
 D.FilesState {id:service;persistViewPreferences:false;path:"/home/fixture/Documents";viewMode:"list"}
 D.FilesWindow {id:files;shell:stub;service:service}
 D.CalendarWindow {id:calendar;shell:stub}
 Component.onCompleted:calendar.calendarState.active=false
 function walk(item,seen){if(!item || seen.includes(item))return [];seen.push(item);let found=[item];for(const child of Array.from(item.children || []).concat(Array.from(item.resources || []))){found=found.concat(walk(child,seen));}if(item.contentItem)found=found.concat(walk(item.contentItem,seen));return found;}
 function visible(item,stop){while(item && item!==stop){if(!item.visible)return false;item=item.parent;}return item===stop;}
 function field(window,hint){return walk(window.contentItem,[]).find(item=>item.placeholderText===hint && visible(item,window.contentItem));}
 IpcHandler {target:"ownedText"
  function ready():string {return "ready";}
  function showConsumer(name:string):void {files.visible=name==="Files";calendar.visible=name==="Calendar";if(name==="Files")service.navigate(service.path);else {calendar.calendarState.active=false;calendar.calendarState.loaded=true;calendar.calendarState.providers=[{name:"Private",calendars:[{name:"Calendar collection"}]}];}Qt.callLater(()=>{const w=name==="Files"?files:calendar;w.fitScreen();w.contentItem.Window.window.requestActivate();w.contentItem.children.find(item=>item.visible && item.width===w.width && item.height===w.height).forceActiveFocus();});}
  function focusLastDay():void {const days=walk(calendar.contentItem,[]).filter(item=>item.day!==undefined && item.inMonth!==undefined);days[days.length-1].forceActiveFocus();}
  function runFocusCancellation(kind:string):void {
   audit.cancellationResult="pending";
   const origin=walk(calendar.contentItem,[]).find(item=>item.day!==undefined && item.inMonth===true);origin.forceActiveFocus();
   calendar.chooseAndFocusDay(new Date(2026,9,23));
   let target;
   if(kind==="view"){calendar.view="Year";target=field(calendar,"Search");target.forceActiveFocus();}
   else if(kind==="window"){calendar.visible=false;files.visible=true;files.contentItem.Window.window.requestActivate();target=field(files,"Filter folder or tag:red");target.forceActiveFocus();}
   else {target=field(calendar,"Search");target.forceActiveFocus();}
   Qt.callLater(()=>{audit.cancellationResult=(target.activeFocus || (kind==="window" && !calendar.visible && target.focus))?"passed":"failed";});
  }
  function cancellation():string {return audit.cancellationResult;}
  function resetCalendar():void {calendar.view="Month";calendar.choose(new Date(2026,9,31));}
  function inspect():void {calendar.inspector=true;}
  function state(name:string):string {const w=name==="Files"?files:calendar,native=w.contentItem.Window.window,f=native?native.activeFocusItem:null,search=field(w,name==="Files"?"Filter folder or tag:red":"Search"),entry=f && f.placeholderText==="Folder path"?f:field(w,"Folder path"),view=walk(calendar.contentItem,[]).find(item=>item.objectName==="calendar-content-viewport"),day=f && f.day!==undefined?f:null,point=day?day.mapToItem(view,0,0):null;const fit=walk(w.contentItem,[]).find(item=>item.ownerWindow===w && item.completedRequests!==undefined);const primary=walk(w.contentItem,[]).find(item=>item.text===(name==="Files"?"An ordinary document.txt":"Calendars") && item.fontInfo);return JSON.stringify({resolvedFamily:primary?primary.fontInfo.family:"",fit:fit?{completed:fit.completedRequests,busy:fit.busy,error:fit.error}:null,title:w.title,width:w.width,height:w.height,textScale:D.Theme.textScale,active:!!native && native.active,focus:f?{type:String(f),objectName:f.objectName,parentType:String(f.parent),parentName:f.parent?f.parent.objectName:"",text:f.text || "",hint:f.placeholderText || "",date:day?calendar.calendarState.iso(day.day):null}:null,search:search?{text:search.text,focused:search.activeFocus}:null,entry:entry?{focused:entry.activeFocus,text:entry.text}:null,rows:service.rows.map(row=>row.name),path:service.path,inspector:calendar.inspector,selected:calendar.calendarState.iso(calendar.selected),weeks:calendar.calendarState.weekCount,dayRevealed:point?point.x>=-1 && point.y>=-1 && point.x+day.width<=view.width+1 && point.y+day.height<=view.height+1:null});}
 }
}''')
config=out/'sway.conf';config.write_text('font sans 10\noutput HEADLESS-1 resolution 800x600\noutput * bg #29445b solid_color\nfor_window [app_id="org.quickshell"] floating enable, border pixel 0\ndefault_floating_border pixel 0\n')
processes=[];keyboard=None;records=[]
def launch(argv,name):
 with (out/name).open('w')as log:child=sp.Popen(argv,env=env,stdout=log,stderr=sp.STDOUT,start_new_session=True)
 processes.append(child);return child
def call(*argv):return sp.check_output(argv,env=env,text=True,timeout=5).strip()
def ipc(method,*args):return call('quickshell','-p',str(qml),'ipc','call','ownedText',method,*args)
def state(name):return json.loads(ipc('state',name))
def tree():return json.loads(call('swaymsg','-r','-t','get_tree'))
def nodes(node):
 yield node
 for child in node.get('nodes',[])+node.get('floating_nodes',[]):yield from nodes(child)
def wait(fn,label):
 deadline=time.monotonic()+10
 while time.monotonic()<deadline:
  try:value=fn()
  except sp.CalledProcessError:value=None
  if value:return value
  time.sleep(.03)
 (out/'failure.json').write_text(json.dumps(dict(label=label,records=records,states={name:state(name) for name in ('Files','Calendar')},tree=tree()),indent=2));raise AssertionError(label)
def command(value):
 response=json.loads(call('swaymsg','-r',value));assert all(row.get('success')for row in response),response
try:
 start_private_bus(env,out,processes);launch([str(a.sway),'-d','--unsupported-gpu','-c',str(config)],'compositor.log');wait(lambda:list(Path(env['XDG_RUNTIME_DIR']).glob('sway-ipc.*.sock')),'private compositor');env['SWAYSOCK']=str(next(Path(env['XDG_RUNTIME_DIR']).glob('sway-ipc.*.sock')));env['WAYLAND_DISPLAY']=next(path.name for path in Path(env['XDG_RUNTIME_DIR']).glob('wayland-*')if not path.name.endswith('.lock'));command('lumen_window margins disable');shell_process=launch(['quickshell','-p',str(qml)],'shell.log')
 with (out/'keyboard.log').open('w')as log:keyboard=PhysicalKeyboard(a.keyboard,env,log)
 processes.append(keyboard.process)
 wait(lambda:ipc('ready')=='ready','private owned IPC ready')
 for mode in ('light','dark'):
  theme(mode)
  for name in ('Files','Calendar'):
   ipc('showConsumer',name);node=wait(lambda:next((row for row in nodes(tree())if row.get('app_id')=='org.quickshell' and row.get('pid')==shell_process.pid and row.get('name')==state(name)['title']),None),'exact owned window');wait(lambda:state(name)['fit'] and state(name)['fit']['completed']>0 and not state(name)['fit']['busy'],'production screen fit ready');node=next(row for row in nodes(tree())if row.get('id')==node['id']);padding_width=node['rect']['width']-node['window_rect']['width'];padding_height=node['rect']['height']+node['deco_rect']['height']-node['window_rect']['height'];command('[con_id='+str(node['id'])+'] resize set width '+str(480+padding_width)+' px height '+str(320+padding_height)+' px, focus');wait(lambda:state(name)['active'] and state(name)['width']==480 and state(name)['height']==320 and state(name)['textScale']==1.5 and state(name)['resolvedFamily']==(a.expected_resolved_family or palette['font']),'real small focused window')
   keyboard.key('-M','ctrl','-k','f','-m','ctrl');wait(lambda:state(name)['search'] and state(name)['search']['focused'],'physical search focus')
   if name=='Files':
    keyboard.key('-M','ctrl','-k','a','-m','ctrl');keyboard.type('ordinary');wait(lambda:state(name)['search']['text']=='ordinary' and state(name)['rows']==['An ordinary document.txt'],'physical filter');keyboard.key('-M','ctrl','-k','l','-m','ctrl');wait(lambda:state(name)['entry']is not None,'real go-to popup');keyboard.key('-k','Escape');wait(lambda:state(name)['entry']is None,'physical cancel');assert state(name)['path']==str(folder)
   else:
    ipc('focusLastDay');wait(lambda:state(name)['dayRevealed']is True,'focused last date revealed');before=state(name)['focus']['date'];keyboard.key('-k','Left');wait(lambda:state(name)['focus']['date']is not None and state(name)['focus']['date']!=before and state(name)['selected']==state(name)['focus']['date'] and state(name)['dayRevealed']is True,'physical calendar date navigation');before_weeks=state(name)['weeks'];keyboard.key('-k','Right');wait(lambda:state(name)['selected']=='2026-11-01' and state(name)['focus']['date']=='2026-11-01' and state(name)['dayRevealed']is True,'forward month boundary keeps captured date focus');assert state(name)['weeks']!=before_weeks;keyboard.key('-k','Left');wait(lambda:state(name)['selected']=='2026-10-31' and state(name)['focus']['date']=='2026-10-31' and state(name)['dayRevealed']is True,'backward month boundary keeps captured date focus');assert state(name)['weeks']==before_weeks
    for kind in ('view','window','search'):
     ipc('runFocusCancellation',kind);wait(lambda:ipc('cancellation')!='pending','queued date focus completes');assert ipc('cancellation')=='passed',kind
     if kind=='window':
      wait(lambda:state('Files')['active'],'Files active after hiding Calendar');keyboard.key('-M','ctrl','-k','f','-m','ctrl');wait(lambda:state('Files')['search']['focused'],'replacement window retains reachable focus')
     ipc('resetCalendar');ipc('showConsumer','Calendar');wait(lambda:state('Calendar')['active'],'Calendar active after scope test')
    ipc('inspect');wait(lambda:state(name)['inspector'],'actual inspector');keyboard.key('-k','Escape');wait(lambda:not state(name)['inspector'],'physical inspector cancel')
   call('grim','-o','HEADLESS-1',str(out/(mode+'-'+name.lower()+'.png')));records.append(dict(mode=mode,consumer=name,state=state(name),checks=8 if name=='Calendar' else 3))
 assert (folder/'An ordinary document.txt').read_text()=='Private fixture unchanged\n';text=(out/'shell.log').read_text();assert not re.search(r'ReferenceError|TypeError|binding loop',text),text
 (out/'result.json').write_text(json.dumps(dict(passed=True,cases=records,scope='Actual isolated production small windows at150%, physical search/filter/date navigation/Escape; no reference-exact or external application claims'),indent=2))
finally:
 if keyboard:
  keyboard.close()
 for child in reversed(processes):
  if child.poll()is None:
   os.killpg(child.pid,signal.SIGTERM)
   try:child.wait(timeout=5)
   except sp.TimeoutExpired:os.killpg(child.pid,signal.SIGKILL);child.wait(timeout=5)
 close_reaped_pipes(processes)

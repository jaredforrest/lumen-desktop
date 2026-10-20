"""Production calendar details keyboard acceptance in private namespaces."""
import argparse,hashlib,json,os,re,shutil,signal,subprocess as sp,sys,time
from pathlib import Path
from application_fixture import isolation
from application_fixture.keyboard import PhysicalKeyboard
from private_native_bus import start_private_bus
from fixture_resources import close_reaped_pipes
p=argparse.ArgumentParser();p.add_argument('--launcher',type=Path,required=True);p.add_argument('--sway',type=Path,required=True);p.add_argument('--keyboard',type=Path,required=True);p.add_argument('--font-config',type=Path,required=True);p.add_argument('--software-driver',type=Path,required=True);p.add_argument('--effects',action='store_true');p.add_argument('--inside',action='store_true');p.add_argument('--output',type=Path,required=True);a=p.parse_args();repo=Path(__file__).resolve().parents[2]
for name in ('launcher','sway','keyboard','font_config','software_driver'):
 value=getattr(a,name).resolve();assert str(value).startswith('/nix/store/')and value.exists(),name;setattr(a,name,value)
if not a.inside:
 assert not sp.check_output(['git','-C',str(repo),'status','--porcelain'],text=True).strip(),'Freeze source before acceptance'
 revision=sp.check_output(['git','-C',str(repo),'rev-parse','HEAD'],text=True).strip();out=a.output.resolve();out.mkdir(mode=0o700,parents=True,exist_ok=False);prefix=re.split(r'\n[ \t]*exec quickshell ',a.launcher.read_text(),maxsplit=1)[0];env=dict(v.decode().split('=',1)for v in sp.check_output(['bash','-c',prefix+'\nexec env -0']).split(b'\0')if b'='in v);env['FONTCONFIG_FILE']=str(a.font_config)
 command=[sys.executable,'/source/tests/desktop/calendar_detail_typography_native.py','--inside','--output','/artifacts']+sum(([ '--'+name.replace('_','-'),str(getattr(a,name))]for name in ('launcher','sway','keyboard','font_config','software_driver')),[])+(['--effects']if a.effects else []);source_hash=hashlib.sha256(Path(__file__).read_bytes()).hexdigest();code=isolation.run(repo,out,command,env,software_graphics=a.effects);unchanged=sp.check_output(['git','-C',str(repo),'rev-parse','HEAD'],text=True).strip()==revision and not sp.check_output(['git','-C',str(repo),'status','--porcelain'],text=True).strip();(out/'provenance.json').write_text(json.dumps(dict(revision=revision,sourceSha256=source_hash,sourceUnchanged=unchanged,exitCode=code,launcher=str(a.launcher),compositor=str(a.sway),fontConfig=str(a.font_config)),indent=2));assert unchanged;raise SystemExit(code)
assert repo==Path('/source')and os.environ['HOME']=='/home/fixture'and os.getuid()!=0 and not Path('/.old-root').exists();assert not any((Path('/dev')/name).exists()for name in ('dri','snd','video0','tty'))
out=a.output;env=dict(os.environ,FONTCONFIG_FILE=str(a.font_config),LANG='C.UTF-8',LC_ALL='C.UTF-8',WLR_BACKENDS='headless',WLR_HEADLESS_OUTPUTS='1',WLR_RENDERER='gles2'if a.effects else 'pixman',WLR_RENDERER_FORCE_SOFTWARE='1'if a.effects else '0',WLR_RENDERER_ALLOW_SOFTWARE='1',WLR_LIBINPUT_NO_DEVICES='1',LIBGL_ALWAYS_SOFTWARE='1',MESA_LOADER_DRIVER_OVERRIDE='llvmpipe',LIBGL_DRIVERS_PATH=str(a.software_driver/'lib/dri'),__EGL_VENDOR_LIBRARY_FILENAMES=str(a.software_driver/'share/glvnd/egl_vendor.d/50_mesa.json'),QT_QPA_PLATFORM='wayland',QT_QUICK_BACKEND='software',QT_QPA_PLATFORMTHEME='generic',QT_ACCESSIBILITY='0',NO_AT_BRIDGE='1',GTK_A11Y='none',LUMEN_UNIFIED_OWNED_HEADERS='1',LUMEN_NATIVE_WINDOW_CONTROLS='0',LUMEN_NATIVE_WINDOW_ACTIONS='0',LUMEN_FILE_SERVICE='0',LUMEN_LAUNCH_SCOPES='0',LUMEN_NEARBY_SHARING_PROVIDER='',GLOBAL_MENU_PYTHON=sys.executable)
for key in ('DISPLAY','WAYLAND_DISPLAY','SWAYSOCK'):env.pop(key,None)
assets=out/'desktop';shutil.copytree(repo/'res/lumen-desktop/shell',assets);palette=json.loads((repo/'res/lumen-desktop/appearance/palette.json').read_text());appearance=out/'appearance.json';env['LUMEN_APPEARANCE_STATE']=str(appearance);appearance.write_text(json.dumps(dict(font=palette['font'],palette=palette['light'],preferences=dict(textScale=1.5,reduceMotion=True,reduceTransparency=True))))
binpath=out/'bin';binpath.mkdir();env['PATH']=str(binpath)+':'+env['PATH'];actions=out/'provider-actions.jsonl';event=dict(id='private-event',title='Private appointment',date='2026-09-28',endDate='2026-09-28',startTime='09:30',endTime='10:45',provider='Google',calendar='Private calendar',location='Room 42',notes='Private notes for the selected appointment.',editable=True,allDay=False,editStart='2026-09-28T09:30',editEnd='2026-09-28T10:45',accountGeneration='private-generation',recurring=True,calendarId='private',eventId='private-event');payload=dict(events=[event],days=['2026-09-28'],total=1,configured=True,providers=[dict(id='google',name='Private provider',generation='private-generation',configured=True,calendars=[dict(id='private',name='Private calendar',accessRole='owner')])])
provider=binpath/'lumen-calendar';provider.write_text('#!'+sys.executable+'\nimport json,sys\nr=json.loads(sys.stdin.readline())\nwith open('+repr(str(actions))+',"a")as f:f.write(json.dumps(r)+"\\n")\nassert r.get("action")in("list","load"),"Read-only private provider"\nprint('+repr(json.dumps(payload))+')\n');provider.chmod(0o755)
qml=out/'fixture.qml';qml.write_text('''import QtQuick
import QtQuick.Controls
import QtQuick.Window
import Quickshell
import Quickshell.Io
import "desktop" as D
ShellRoot {
 id:audit
 property var palettes:'''+json.dumps(palette)+'''
 property string wanted:""
 QtObject {id:stub;property date currentDate:new Date(2026,8,28);property int accounts:0;function claimPopup(name){}function focusOwnedWindow(name){}function openSettings(name){accounts++;}}
 D.CalendarWindow {id:owner;shell:stub}
 function walk(item,seen){if(!item || seen.includes(item))return [];seen.push(item);let result=[item];for(const child of Array.from(item.children || []).concat(Array.from(item.resources || [])))result=result.concat(walk(child,seen));if(item.contentItem)result=result.concat(walk(item.contentItem,seen));return result;}
 function shown(item){while(item && item!==owner.contentItem){if(!item.visible)return false;item=item.parent;}return item===owner.contentItem;}
 function rect(item){const p=item.mapToItem(owner.contentItem,0,0);return {x:p.x,y:p.y,width:item.width,height:item.height};}
 function focusedName(){for(let item=owner.contentItem.Window.window.activeFocusItem;item;item=item.parent){if(typeof item.mapToItem!=="function")continue;if(item.Accessible.name)return item.Accessible.name;if(item.clicked && item.text)return item.text;}return "";}
 IpcHandler {target:"calendarDetails"
  function ready():bool{return true;}
  function open(mode:string):void {owner.visible=false;owner.inspector=false;owner.calendarState.editing=false;D.Theme.font=palettes.font;D.Theme.colors=palettes[mode];D.Theme.preferences={textScale:1.5,reduceMotion:true,reduceTransparency:true};owner.implicitWidth=480;owner.implicitHeight=320;owner.visible=true;Qt.callLater(()=>owner.contentItem.Window.window.requestActivate());}
  function details():void {owner.eventDetails(owner.calendarState.events[0],new Date(2026,8,28));owner.calendarState.optionsOpen=true;}
  function focus(label:string):void {audit.wanted=label;const target=walk(owner.contentItem,[]).find(item=>typeof item.mapToItem==="function" && shown(item) && ((item.clicked && item.text===label) || item.Accessible.name===label));if(!target)throw Error("Missing actual control "+label);target.forceActiveFocus();}
  function state():string {const backing=owner.contentItem.Window.window;if(!backing)return JSON.stringify({active:false});const items=walk(owner.contentItem,[]).filter(item=>typeof item.mapToItem==="function" && shown(item)),target=wanted?items.find(item=>item.clicked && item.text===wanted && item.contentItem && item.contentItem.font):null;return JSON.stringify({active:backing.active,visible:owner.visible,loaded:owner.calendarState.loaded,busy:owner.calendarState.busy,events:owner.calendarState.events.length,editing:owner.calendarState.editing,inspector:owner.inspector,detail:owner.calendarState.detail?owner.calendarState.detail.id:null,focus:focusedName(),accounts:stub.accounts,width:owner.width,height:owner.height,target:target?{rect:rect(target),textRect:rect(target.contentItem),color:String(target.contentItem.color),pixelSize:target.contentItem.font.pixelSize}:null});}
 }
}''')
config=out/'sway.conf';config.write_text('include /source/res/lumen-desktop/sway.conf\noutput HEADLESS-1 resolution 900x700\nfocus_follows_mouse no\n');processes=[];keyboard=None;records=[]
def launch(command,name,**kwargs):
 with (out/name).open('w')as log:child=sp.Popen(list(map(str,command)),env=env,stdout=log,stderr=sp.STDOUT,start_new_session=True,**kwargs)
 processes.append(child);return child
def call(*command):return sp.check_output(list(map(str,command)),env=env,text=True,timeout=5).strip()
def ipc(method,*args):return call('quickshell','ipc','-p',qml,'call','calendarDetails',method,*args)
def state():return json.loads(ipc('state'))
def wait(predicate,label):
 deadline=time.monotonic()+15
 while time.monotonic()<deadline:
  try:
   if predicate():return
  except (ValueError,sp.SubprocessError):pass
  time.sleep(.03)
 try:last=state()
 except (ValueError,sp.SubprocessError)as error:last=dict(observerError=str(error))
 (out/'failure.json').write_text(json.dumps(dict(label=label,state=last),indent=2));raise AssertionError(label)
def nodes(row):
 yield row
 for child in row.get('nodes',[])+row.get('floating_nodes',[]):yield from nodes(child)
def paint(name):
 from PIL import Image,ImageColor
 receipt=[]
 def painted():
  row=state();r=row['target']['rect'];assert r['x']>=0 and r['y']>=0 and r['x']+r['width']<=row['width']+1 and r['y']+r['height']<=row['height']+1,row;assert row['target']['pixelSize']==20,row
  node=next(item for item in nodes(json.loads(call(a.sway.parent/'swaymsg','-r','-t','get_tree')))if item.get('pid')==shell.pid and item.get('name')=='Calendar');origin=(node['rect']['x']+node['window_rect']['x'],node['rect']['y']+node['window_rect']['y']);r=row['target']['textRect'];box=tuple(round(v)for v in [origin[0]+r['x'],origin[1]+r['y'],origin[0]+r['x']+r['width'],origin[1]+r['y']+r['height']]);path=out/(name+'.png');call('grim','-o','HEADLESS-1',path)
  with Image.open(path)as image:ink=sum(max(abs(a-b)for a,b in zip(pixel,ImageColor.getrgb(row['target']['color'])))<=2 for pixel in image.convert('RGB').crop(box).get_flattened_data())
  receipt[:]=[dict(conId=node['id'],pid=node['pid'],textBox=box,inkPixels=ink)];return ink>=4
 wait(painted,'actual presented '+name);return receipt[0]
try:
 start_private_bus(env,out,processes);launch([a.sway,'-c',config],'sway.log');runtime=Path(env['XDG_RUNTIME_DIR']);wait(lambda:next(runtime.glob('sway-ipc*.sock'),None),'private compositor');env['SWAYSOCK']=str(next(runtime.glob('sway-ipc*.sock')));env['WAYLAND_DISPLAY']=next(path.name for path in runtime.glob('wayland-*')if not path.name.endswith('.lock'));shell=launch(['quickshell','--no-duplicate','-p',qml],'shell.log')
 with (out/'keyboard.log').open('w')as log:keyboard=PhysicalKeyboard(a.keyboard,env,log)
 processes.append(keyboard.process);wait(lambda:ipc('ready')=='true','private IPC')
 for mode in ['light','dark']:
  ipc('open',mode);wait(lambda:state()['active']and state()['loaded']and not state()['busy']and state()['events']==1,'actual Calendar ready');ipc('details');wait(lambda:state()['detail']=='private-event'and state()['inspector'],'captured details visible');ipc('focus','New Event…');wait(lambda:state()['focus']=='New Event…','actual constrained New Event focus');receipt=paint(mode+'-new-event');keyboard.key('-k','Return');wait(lambda:state()['editing']and state()['focus']=='Event title','physical New Event enters editor');records.append(dict(mode=mode,case='new-event',painted=receipt,state=state()))
  ipc('focus','Cancel');wait(lambda:state()['focus']=='Cancel','actual editor Cancel');keyboard.key('-k','Return');wait(lambda:not state()['editing'],'physical Cancel returns to details');assert state()['detail']=='private-event';records.append(dict(mode=mode,case='cancel',state=state()))
  ipc('focus','Edit Occurrence…');wait(lambda:state()['focus']=='Edit Occurrence…','actual captured event edit');receipt=paint(mode+'-edit-event');keyboard.key('-k','Return');wait(lambda:state()['editing']and state()['focus']=='Event title','physical Edit enters selected occurrence');keyboard.key('-k','Escape');wait(lambda:not state()['editing']and not state()['inspector'],'physical Escape closes inspector');records.append(dict(mode=mode,case='edit-escape',painted=receipt,state=state()))
  ipc('details');ipc('focus','iCloud Accounts…');wait(lambda:state()['focus']=='iCloud Accounts…','actual account row');keyboard.key('-k','Tab');wait(lambda:state()['focus']=='Google Accounts…','physical Tab reaches adjacent account action');ipc('focus','Google Accounts…');receipt=paint(mode+'-accounts');before=state()['accounts'];keyboard.key('-k','Return');wait(lambda:state()['accounts']==before+1,'physical account action reaches private shell stub');records.append(dict(mode=mode,case='accounts-tab',painted=receipt,state=state()));keyboard.key('-k','Escape');wait(lambda:not state()['inspector'],'physical details Escape')
 assert not any(row.get('action')not in('list','load')for row in map(json.loads,actions.read_text().splitlines()))
 (out/'result.json').write_text(json.dumps(dict(passed=True,cases=records),indent=2))
finally:
 if keyboard:keyboard.close()
 for child in reversed(processes):
  if child.poll()is None:
   os.killpg(child.pid,signal.SIGTERM)
   try:child.wait(timeout=5)
   except sp.TimeoutExpired:os.killpg(child.pid,signal.SIGKILL);child.wait(timeout=5)
 close_reaped_pipes(processes)
 if (out/'shell.log').exists():
  text=(out/'shell.log').read_text();warnings=[line for line in text.splitlines()if 'WARN'in line];path=out/'result.json'
  if path.exists():data=json.loads(path.read_text());data['runtimeWarnings']=warnings;path.write_text(json.dumps(data,indent=2))
  assert not warnings and not re.search(r'ReferenceError|TypeError|binding loop',text),text

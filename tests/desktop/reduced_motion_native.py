"""Normal/reduced motion lifecycle outcomes using private real providers and input."""
import argparse,hashlib,json,os,re,shutil,signal,subprocess as sp,sys,time
from pathlib import Path
import dbus
from application_fixture import isolation
from application_fixture.keyboard import PhysicalKeyboard
from private_native_bus import start_private_bus
from fixture_resources import close_reaped_pipes
p=argparse.ArgumentParser();p.add_argument('--launcher',type=Path,required=True);p.add_argument('--sway',type=Path,required=True);p.add_argument('--keyboard',type=Path,required=True);p.add_argument('--pointer-tool',type=Path,required=True);p.add_argument('--font-config',type=Path,required=True);p.add_argument('--software-driver',type=Path,required=True);p.add_argument('--effects',action='store_true');p.add_argument('--inside',action='store_true');p.add_argument('--output',type=Path,required=True);a=p.parse_args();repo=Path(__file__).resolve().parents[2]
for name in ('launcher','sway','keyboard','pointer_tool','font_config','software_driver'):
 value=getattr(a,name).resolve();assert str(value).startswith('/nix/store/')and value.exists(),name;setattr(a,name,value)
if not a.inside:
 assert not sp.check_output(['git','-C',str(repo),'status','--porcelain'],text=True).strip(),'Freeze source before acceptance'
 revision=sp.check_output(['git','-C',str(repo),'rev-parse','HEAD'],text=True).strip();out=a.output.resolve();out.mkdir(mode=0o700,parents=True,exist_ok=False);prefix=re.split(r'\n[ \t]*exec quickshell ',a.launcher.read_text(),maxsplit=1)[0];env=dict(v.decode().split('=',1)for v in sp.check_output(['bash','-c',prefix+'\nexec env -0']).split(b'\0')if b'='in v);env['FONTCONFIG_FILE']=str(a.font_config)
 command=[sys.executable,'/source/tests/desktop/reduced_motion_native.py','--inside','--output','/artifacts']+sum(([ '--'+name.replace('_','-'),str(getattr(a,name))]for name in ('launcher','sway','keyboard','pointer_tool','font_config','software_driver')),[])+(['--effects']if a.effects else []);source_hash=hashlib.sha256(Path(__file__).read_bytes()).hexdigest();code=isolation.run(repo,out,command,env,software_graphics=a.effects);unchanged=sp.check_output(['git','-C',str(repo),'rev-parse','HEAD'],text=True).strip()==revision and not sp.check_output(['git','-C',str(repo),'status','--porcelain'],text=True).strip();(out/'provenance.json').write_text(json.dumps(dict(revision=revision,sourceSha256=source_hash,sourceUnchanged=unchanged,exitCode=code,launcher=str(a.launcher),compositor=str(a.sway),fontConfig=str(a.font_config)),indent=2));assert unchanged;raise SystemExit(code)
assert repo==Path('/source')and os.environ['HOME']=='/home/fixture'and os.getuid()!=0 and not Path('/.old-root').exists();assert not any((Path('/dev')/name).exists()for name in ('dri','snd','video0','tty'))
out=a.output;env=dict(os.environ,FONTCONFIG_FILE=str(a.font_config),LANG='C.UTF-8',LC_ALL='C.UTF-8',WLR_BACKENDS='headless',WLR_HEADLESS_OUTPUTS='1',WLR_RENDERER='gles2'if a.effects else'pixman',WLR_RENDERER_FORCE_SOFTWARE='1'if a.effects else'0',WLR_RENDERER_ALLOW_SOFTWARE='1',WLR_LIBINPUT_NO_DEVICES='1',LIBGL_ALWAYS_SOFTWARE='1',MESA_LOADER_DRIVER_OVERRIDE='llvmpipe',LIBGL_DRIVERS_PATH=str(a.software_driver/'lib/dri'),__EGL_VENDOR_LIBRARY_FILENAMES=str(a.software_driver/'share/glvnd/egl_vendor.d/50_mesa.json'),QT_QPA_PLATFORM='wayland',QT_QUICK_BACKEND='software',QT_QPA_PLATFORMTHEME='generic',QT_ACCESSIBILITY='0',NO_AT_BRIDGE='1',GTK_A11Y='none',LUMEN_UNIFIED_OWNED_HEADERS='1',LUMEN_NATIVE_WINDOW_CONTROLS='0',LUMEN_NATIVE_WINDOW_ACTIONS='0',LUMEN_LAUNCH_SCOPES='0',LUMEN_LOCK_PREVIEWS='0',GLOBAL_MENU_PYTHON=sys.executable)
for key in ('DISPLAY','WAYLAND_DISPLAY','SWAYSOCK'):env.pop(key,None)
assets=out/'shell';shutil.copytree(repo/'res/lumen-desktop/shell',assets);notifications_source=repo/'res/lumen-desktop/notifications';shutil.copytree(notifications_source,out/'notifications');env['LUMEN_NOTIFICATION_HELPER']=str(out/'notifications/service.py');env['LUMEN_NOTIFICATION_STATE']=str(out/'notification-history');policy=out/'notification-policy.json';policy.write_text(json.dumps(dict(notifications=dict(preview='when_unlocked',policies={}))));env['LUMEN_NIX_SETTINGS_ACTIVE']=str(policy);palette=json.loads((repo/'res/lumen-desktop/appearance/palette.json').read_text());appearance=out/'appearance.json';env['LUMEN_APPEARANCE_STATE']=str(appearance)
def preference(reduced):
 pending=appearance.with_suffix('.pending');pending.write_text(json.dumps(dict(font=palette['font'],palette=palette['light'],preferences=dict(reduceMotion=reduced,reduceTransparency=True,dockAutoHide=True,dockMagnification=True,dockSize=48))));pending.replace(appearance)
preference(False);qml=out/'fixture.qml';qml.write_text('''import QtQuick
import QtQuick.Controls
import QtQuick.Window
import Quickshell
import Quickshell.Io
import "shell" as D
ShellRoot {
 id:audit
 property var actions:[]
 property int confirmations:0
 property bool tracing:false
 property bool traceOverflow:false
 property var trace:[]
 QtObject {id:stub
  property string popupOwner:""
  property var bars:({})
  property date currentDate:new Date(2026,9,20)
  property bool dockPreferencesBusy:false
  property string aliasError:""
  property string applicationCommandError:""
  property var referenceDock:[{key:"help",action:"fixture",image:"files.svg",canvas:68},{key:"other",action:"fixture",image:"calendar.svg",canvas:68}]
  property alias notifications:notifications
  property string hudOutput:"HEADLESS-1"
  property bool hudVisible:false
  property var audioState:({available:true,muted:false,level:0.5})
  signal requestDockFocus(string name)
  signal dismissPopups(string owner)
  function claimPopup(owner){popupOwner=owner;dismissPopups(owner);}
  function preferredOutput(){return "HEADLESS-1";}
  function dockWindows(item){return [];}
  function dockLabel(item){return item.key;}
  function runningFor(entry){return false;}
  function activateDock(item,shift,name){audit.actions=audit.actions.concat(item.key);if(item.key==="help")help.show();}
  function canConfigureDockEntry(entry){return false;}
  function isDockPinned(entry){return false;}
  function showMenu(name,output){}
 }
 D.HelpWindow {id:help;shell:stub}
 D.SessionOptions {id:session;executeActions:false;onConfirmed:audit.confirmations++}
 D.NotificationState {id:notifications;shell:stub}
 D.NotificationBanners {id:banners;shell:stub;outputName:"HEADLESS-1";outputWidth:1100;outputHeight:800}
 D.Dock {id:dock;shell:stub}
 D.VolumeHud {id:hud;shell:stub;outputName:"HEADLESS-1"}
 function walk(item,seen){if(!item || seen.includes(item))return [];seen.push(item);let found=[item];for(const child of Array.from(item.children || []).concat(Array.from(item.resources || [])))found=found.concat(walk(child,seen));for(const key of ["contentItem","animation"])if(item[key])found=found.concat(walk(item[key],seen));return found;}
 function rect(item,owner){const point=item.mapToItem(owner.contentItem,0,0);return {x:point.x,y:point.y,width:item.width,height:item.height};}
 function icons(){return walk(dock.contentItem,[]).filter(item=>item.iconSource!==undefined && item.parent && item.parent.magnification!==undefined);}
 Timer {interval:4;repeat:true;running:audit.tracing;onTriggered:{if(audit.trace.length>=1024){audit.traceOverflow=true;audit.tracing=false;return;}const card=audit.walk(banners.contentItem,[]).find(item=>item.objectName==="notification-card");audit.trace=audit.trace.concat({widths:audit.icons().map(item=>item.width),banner:card?card.opacity:null});}}

 IpcHandler {target:"motionAudit"
  function beginTrace():void {audit.trace=[];audit.traceOverflow=false;audit.tracing=true;}
  function endTrace():string {audit.tracing=false;return JSON.stringify({overflow:audit.traceOverflow,samples:audit.trace});}
  function focusDock():void {stub.requestDockFocus("HEADLESS-1");}
  function showSession(action:string):void {session.show(action);}
  function showHud(value:bool):void {stub.hudVisible=value;}
  function state():string {
   const items=walk(banners.contentItem,[]),card=items.find(item=>item.objectName==="notification-card"),summary=card?walk(card,[]).find(item=>item.text===card.notification.summary && item.font):null,dismiss=card?walk(card,[]).find(item=>item.objectName==="notification-dismiss"):null;
   const icons=audit.icons();
   const surface=walk(dock.contentItem,[]).find(item=>item!==dock.contentItem && String(item).startsWith("QQuickItem") && item.width===dock.width && item.height===dock.height && item.opacity!==undefined),hudSurface=walk(hud.contentItem,[]).find(item=>item.radius===22 && item.opacity!==undefined),search=walk(help.contentItem,[]).find(item=>item.placeholderText==="Search Help");
   return JSON.stringify({reduced:!!D.Theme.preferences.reduceMotion,help:{visible:help.visible,active:!!help.contentItem.Window.window && help.contentItem.Window.window.active,query:help.query,search:!!search && search.activeFocus,width:help.width,height:help.height},session:{visible:session.visible,active:!!session.contentItem.Window.window && session.contentItem.Window.window.active,focus:session.contentItem.Window.window && session.contentItem.Window.window.activeFocusItem?session.contentItem.Window.window.activeFocusItem.Accessible.name:"",confirmations:audit.confirmations},dock:{active:!!dock.contentItem.Window.window && dock.contentItem.Window.window.active,keyboard:dock.keyboardMode,index:dock.keyboardIndex,hovered:dock.hovered,revealed:dock.revealed,surfaceOpacity:surface?surface.opacity:null,icons:icons.map(item=>({key:item.parent.modelData.key,width:item.width,target:item.parent.modelData.canvas*dock.artworkScale+18*item.parent.magnification}))},hud:{visible:hud.visible,opacity:hudSurface?hudSurface.opacity:null,width:hud.width,height:hud.height,bottom:hud.margins.bottom},notifications:{available:notifications.available,rows:notifications.rows.map(row=>({id:row.id,summary:row.summary,body:row.body,banner:row.banner})),banners:notifications.banners.length,visible:banners.visible,opacity:card?card.opacity:null,width:banners.width,height:banners.height,right:banners.margins.right,top:banners.margins.top,summary:summary?{rect:rect(summary,banners),color:String(summary.color)}:null,dismiss:dismiss?{visible:dismiss.visible,rect:rect(dismiss,banners)}:null},actions:audit.actions});
  }
 }
}''')
config=out/'sway.conf';config.write_text('include /source/res/lumen-desktop/sway.conf\noutput HEADLESS-1 resolution 1100x800\noutput * bg #29445b solid_color\nfocus_follows_mouse no\nbindsym Mod4+1 workspace number 1\nbindsym Mod4+2 workspace number 2\n')
processes=[];keyboard=None;pointer=None;connection=None;records=[]
def launch(command,name,**kw):
 with (out/name).open('w')as log:process=sp.Popen(list(map(str,command)),env=env,stdout=log,stderr=sp.STDOUT,start_new_session=True,**kw)
 processes.append(process);return process
def call(*command):return sp.check_output(list(map(str,command)),env=env,text=True,timeout=5).strip()
def nodes(node):
 yield node
 for child in node.get('nodes',[])+node.get('floating_nodes',[]):yield from nodes(child)
def tree():return json.loads(call(a.sway.parent/'swaymsg','-r','-t','get_tree'))
def ipc(method,*args):return call('quickshell','ipc','-p',qml,'call','motionAudit',method,*args)
def state():return json.loads(ipc('state'))
def wait(predicate,label):
 deadline=time.monotonic()+15
 while time.monotonic()<deadline:
  try:
   value=predicate()
   if value:return value
  except (OSError,ValueError,sp.SubprocessError,StopIteration,dbus.DBusException):pass
  time.sleep(.03)
 try:(out/'failure.json').write_text(json.dumps(dict(label=label,records=records,state=state(),tree=tree()),indent=2))
 except Exception:pass
 raise AssertionError(label)
def command(value):
 result=json.loads(call(a.sway.parent/'swaymsg','-r',value));assert all(row.get('success')for row in result),result
def capture(name):
 path=out/(name+'.png');call('grim','-o','HEADLESS-1',path);return path
def point(x,y,click=False):
 command('seat seat0 cursor set '+str(round(x))+' '+str(round(y)));pointer.stdin.write('m 0 0\n'+('p\nr\n'if click else''));pointer.stdin.flush()
def close(process):
 if process.poll()is None:os.killpg(process.pid,signal.SIGTERM);process.wait(timeout=5)
def help_node():return next((row for row in nodes(tree())if row.get('pid')==shell_process.pid and row.get('name')=='Desktop Help'),None)
def icons_settled():return bool(state()['dock']['icons'])and all(abs(row['width']-row['target'])<.01 for row in state()['dock']['icons'])
try:
 start_private_bus(env,out,processes);connection=dbus.bus.BusConnection(env['DBUS_SESSION_BUS_ADDRESS']);launch([a.sway,'-c',config],'sway.log');runtime=Path(env['XDG_RUNTIME_DIR']);env['SWAYSOCK']=str(wait(lambda:next(runtime.glob('sway-ipc*.sock'),None),'private compositor'));env['WAYLAND_DISPLAY']=wait(lambda:next((path.name for path in runtime.glob('wayland-*')if not path.name.endswith('.lock')),None),'private Wayland display');launch([sys.executable,env['LUMEN_NOTIFICATION_HELPER'],'serve'],'notifications.log')
 def api(interface='org.freedesktop.Notifications'):return dbus.Interface(connection.get_object('org.freedesktop.Notifications','/org/freedesktop/Notifications'),interface)
 wait(lambda:json.loads(api('org.lumendesktop.Notifications').Snapshot(timeout=5))['lock']==dict(known=True,locked=False),'private unlocked observer')
 with (out/'keyboard.log').open('w')as log:keyboard=PhysicalKeyboard(a.keyboard,env,log)
 processes.append(keyboard.process);pointer=launch([a.pointer_tool,'pointer','move','0','0'],'pointer.log',stdin=sp.PIPE,text=True)
 from PIL import Image,ImageColor,ImageChops
 for reduced in (False,True):
  label='reduced'if reduced else'normal';preference(reduced)
  if a.effects:command('animation_duration_ms '+('0'if reduced else'160'))
  shell_process=launch(['quickshell','--no-duplicate','-p',qml],label+'-shell.log');wait(lambda:state()['reduced']==reduced and state()['notifications']['available'],'actual motion preference and notification transport');wait(lambda:not state()['dock']['revealed'] and state()['dock']['surfaceOpacity']==0,'initial auto-hide completes');base_widths=[row['width']for row in state()['dock']['icons']];ipc('beginTrace');ipc('focusDock');wait(lambda:state()['dock']['active'] and state()['dock']['keyboard'] and state()['dock']['surfaceOpacity']==1 and icons_settled(),'Dock reveal and magnification settle');dock_trace=json.loads(ipc('endTrace'));assert not dock_trace['overflow'] and dock_trace['samples'];target_widths=[row['target']for row in state()['dock']['icons']];intermediate=any(any(min(base,target)+.01<width<max(base,target)-.01 for width,base,target in zip(row['widths'],base_widths,target_widths))for row in dock_trace['samples']);assert intermediate is not reduced,(reduced,dock_trace);keyboard.key('-k','End');wait(lambda:state()['dock']['index']==1 and icons_settled(),'physical Dock navigation completes');keyboard.key('-k','Escape');wait(lambda:not state()['dock']['keyboard'] and icons_settled(),'physical Dock cancel');assert state()['actions']==[]
  ipc('focusDock');wait(lambda:state()['dock']['active']and state()['dock']['keyboard']and state()['dock']['surfaceOpacity']==1,'Dock reopen');keyboard.key('-k','Home');keyboard.key('-k','Return');wait(lambda:help_node()and state()['help']['active']and state()['help']['search'],'physical Dock activates actual Help');assert state()['actions']==['help'];keyboard.type('Dock');wait(lambda:state()['help']['query']=='Dock','physical application input after motion');keyboard.key('-k','Escape');wait(lambda:not state()['help']['visible']and not help_node(),'physical application close')
  ipc('focusDock');wait(lambda:state()['dock']['active']and state()['dock']['keyboard'],'Dock third open');keyboard.key('-k','Home');keyboard.key('-k','Return');wait(lambda:help_node()and state()['help']['active'],'actual Help reopen');assert state()['help']['query']=='Dock';node=help_node();captured=node['id'];command('[con_id='+str(captured)+'] lumen_window minimize');wait(lambda:next(row for row in nodes(tree())if row.get('id')==captured).get('lumen_state',{}).get('minimised')is True,'native minimize completes');command('[con_id='+str(captured)+'] scratchpad show');wait(lambda:help_node()and state()['help']['active']and not help_node().get('lumen_state',{}).get('minimised'),'captured native restore completes')
  command('[con_id='+str(captured)+'] move container to workspace number 2');keyboard.key('-M','logo','-k','2','-m','logo');wait(lambda:any(row.get('name')=='2'and row.get('focused')for row in json.loads(call(a.sway.parent/'swaymsg','-r','-t','get_workspaces'))),'physical workspace2 completion');keyboard.key('-M','logo','-k','1','-m','logo');wait(lambda:any(row.get('name')=='1'and row.get('focused')for row in json.loads(call(a.sway.parent/'swaymsg','-r','-t','get_workspaces'))),'physical workspace1 completion');command('[con_id='+str(captured)+'] move container to workspace number 1, focus');wait(lambda:state()['help']['active'],'application focus after workspace return')
  for action in ('suspend','reboot','poweroff','logout'):
   for key in ('Return','Escape'):
    ipc('showSession',action);wait(lambda:state()['session']['active'] and state()['session']['focus']=='Cancel','actual default Cancel focus '+action);keyboard.key('-k',key);wait(lambda:not state()['session']['visible'],'physical session Cancel '+action+' '+key);assert state()['session']['confirmations']==0,(action,key,state()['session'])
  ipc('showSession','suspend');wait(lambda:state()['session']['active']and state()['session']['focus']=='Cancel','Sleep cancellation default');keyboard.key('-k','Tab');wait(lambda:state()['session']['focus']=='Sleep','deliberately focused Sleep');keyboard.key('-k','Return');wait(lambda:not state()['session']['visible'],'physical intended Sleep confirmation');assert state()['session']['confirmations']==1,state()['session']
  command('[con_id='+str(captured)+'] focus');wait(lambda:state()['help']['active'],'Help active after cancellation');before=capture(label+'-hud-before');ipc('showHud','true');wait(lambda:state()['hud']['visible']and state()['hud']['opacity']==1,'HUD opacity completes');shown=capture(label+'-hud-shown');hud_state=state()['hud'];box=(round((1100-hud_state['width'])/2),round(800-hud_state['bottom']-hud_state['height']),round((1100+hud_state['width'])/2),round(800-hud_state['bottom']));
  with Image.open(before)as first,Image.open(shown)as second:assert ImageChops.difference(first.convert('RGB').crop(box),second.convert('RGB').crop(box)).getbbox(),'Actual HUD must paint'
  ipc('showHud','false');wait(lambda:not state()['hud']['visible'],'HUD hide completes')
  ipc('beginTrace');ident=int(api().Notify('Private motion producer',0,'','Motion receipt','Private synthetic body',[],{},0,timeout=5));wait(lambda:state()['notifications']['banners']==1 and state()['notifications']['opacity']==1,'actual provider banner opacity completes');arrival=state();banner_trace=json.loads(ipc('endTrace'));assert not banner_trace['overflow'] and banner_trace['samples'];banner_intermediate=any(row['banner']is not None and .001<row['banner']<.999 for row in banner_trace['samples']);assert banner_intermediate is not reduced,(reduced,banner_trace);summary=arrival['notifications']['summary'];assert summary;origin=(1100-arrival['notifications']['width']-arrival['notifications']['right'],arrival['notifications']['top']);rect=summary['rect'];expected=ImageColor.getrgb(summary['color']);banner_capture=out/(label+'-banner.png')
  def banner_painted():
   capture(label+'-banner')
   with Image.open(banner_capture)as image:
    box=tuple(round(value)for value in(origin[0]+rect['x'],origin[1]+rect['y'],origin[0]+rect['x']+rect['width'],origin[1]+rect['y']+rect['height']));return sum(max(abs(a-b)for a,b in zip(pixel,expected))<=2 for pixel in image.convert('RGB').crop(box).get_flattened_data())>=4
  wait(banner_painted,'actual banner summary presented');assert arrival['notifications']['opacity']==1
  dismiss=arrival['notifications']['dismiss']['rect'];point(origin[0]+dismiss['x']+dismiss['width']/2,origin[1]+dismiss['y']+dismiss['height']/2);wait(lambda:state()['notifications']['dismiss']and state()['notifications']['dismiss']['visible'],'actual dismiss hover');point(origin[0]+dismiss['x']+dismiss['width']/2,origin[1]+dismiss['y']+dismiss['height']/2,True);wait(lambda:all(row['id']!=ident for row in state()['notifications']['rows'])and not state()['notifications']['visible'],'physical provider-backed notification dismiss')
  wait(lambda:icons_settled()and not state()['dock']['keyboard'],'final Dock state settles');settled=state();normalized=dict(helpGeometry=help_node()['rect'],helpActive=settled['help']['active'],workspace=next(row['name']for row in json.loads(call(a.sway.parent/'swaymsg','-r','-t','get_workspaces'))if row.get('focused')),helpVisible=settled['help']['visible'],query=settled['help']['query'],actions=settled['actions'],confirmations=settled['session']['confirmations'],notifications=settled['notifications']['rows'],hudVisible=settled['hud']['visible'],dockKeyboard=settled['dock']['keyboard']);records.append(dict(reduced=reduced,normalized=normalized,dockTrace=dock_trace,bannerTrace=banner_trace,checks=15))
  close(shell_process);shell_process=launch(['quickshell','--no-duplicate','-p',qml],label+'-restart.log');wait(lambda:state()['reduced']==reduced and state()['notifications']['available'],'restart retains preference and reconnects');assert not state()['help']['visible']and not state()['notifications']['rows'];ipc('focusDock');wait(lambda:state()['dock']['active']and state()['dock']['keyboard'],'restart Dock focus');keyboard.key('-k','Home');keyboard.key('-k','Return');wait(lambda:state()['help']['active']and state()['help']['search'],'restart physical activate');keyboard.type('Dock');wait(lambda:state()['help']['query']=='Dock','restart physical input');keyboard.key('-k','Escape');wait(lambda:not state()['help']['visible'],'restart physical close');close(shell_process)
 assert records[0]['normalized']==records[1]['normalized'],records
 for path in out.glob('*-shell.log'):assert not re.search(r'ReferenceError|TypeError|binding loop',path.read_text()),path
 (out/'result.json').write_text(json.dumps(dict(passed=True,cases=records,capabilities=dict(normalReducedFinalStateParity=True,physicalDockNavigationCancelActivate=True,capturedNativeMinimizeRestore=True,physicalWorkspaceSwitch=True,physicalDefaultSessionCancel=True,realNotificationProviderAndPhysicalDismiss=True,hudPresented=True,restartInput=True,menuBarFade=False,screenSaverMotion=False,allNavigationScenarios=False),scope='Isolated engineering lifecycle differential; no reference timings or live desktop changes'),indent=2))
finally:
 if connection:connection.close()
 if keyboard:keyboard.close()
 if pointer and pointer.stdin and not pointer.stdin.closed:pointer.stdin.close()
 for process in reversed(processes):
  if process.poll()is None:
   os.killpg(process.pid,signal.SIGTERM)
   try:process.wait(timeout=5)
   except sp.TimeoutExpired:os.killpg(process.pid,signal.SIGKILL);process.wait(timeout=5)
 close_reaped_pipes(processes)

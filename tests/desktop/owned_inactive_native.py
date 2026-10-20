"""Independent owned clients and utility key ownership with actual painted states."""
import argparse,hashlib,json,os,re,shutil,signal,subprocess as sp,sys,time
from pathlib import Path
from application_fixture import isolation
from application_fixture.keyboard import PhysicalKeyboard
from private_native_bus import start_private_bus
from fixture_resources import close_reaped_pipes
p=argparse.ArgumentParser();p.add_argument('--launcher',type=Path,required=True);p.add_argument('--sway',type=Path,required=True);p.add_argument('--keyboard',type=Path,required=True);p.add_argument('--font-config',type=Path,required=True);p.add_argument('--software-driver',type=Path,required=True);p.add_argument('--frame',choices=['native','unified'],required=True);p.add_argument('--effects',action='store_true');p.add_argument('--inside',action='store_true');p.add_argument('--output',type=Path,required=True);a=p.parse_args();repo=Path(__file__).resolve().parents[2]
for name in ('launcher','sway','keyboard','font_config','software_driver'):
 value=getattr(a,name).resolve();assert str(value).startswith('/nix/store/')and value.exists(),name;setattr(a,name,value)
if not a.inside:
 assert not sp.check_output(['git','-C',str(repo),'status','--porcelain'],text=True).strip(),'Freeze source before acceptance'
 revision=sp.check_output(['git','-C',str(repo),'rev-parse','HEAD'],text=True).strip();out=a.output.resolve();out.mkdir(mode=0o700,parents=True,exist_ok=False);prefix=re.split(r'\n[ \t]*exec quickshell ',a.launcher.read_text(),maxsplit=1)[0];env=dict(v.decode().split('=',1)for v in sp.check_output(['bash','-c',prefix+'\nexec env -0']).split(b'\0')if b'='in v);env['FONTCONFIG_FILE']=str(a.font_config)
 command=[sys.executable,'/source/tests/desktop/owned_inactive_native.py','--inside','--frame',a.frame,'--output','/artifacts']+sum(([ '--'+name.replace('_','-'),str(getattr(a,name))]for name in ('launcher','sway','keyboard','font_config','software_driver')),[])+(['--effects']if a.effects else []);source_hash=hashlib.sha256(Path(__file__).read_bytes()).hexdigest();code=isolation.run(repo,out,command,env,software_graphics=a.effects);unchanged=sp.check_output(['git','-C',str(repo),'rev-parse','HEAD'],text=True).strip()==revision and not sp.check_output(['git','-C',str(repo),'status','--porcelain'],text=True).strip();(out/'provenance.json').write_text(json.dumps(dict(revision=revision,sourceSha256=source_hash,sourceUnchanged=unchanged,exitCode=code,launcher=str(a.launcher),compositor=str(a.sway),fontConfig=str(a.font_config)),indent=2));assert unchanged;raise SystemExit(code)
assert repo==Path('/source')and os.environ['HOME']=='/home/fixture'and os.getuid()!=0 and not Path('/.old-root').exists();assert not any((Path('/dev')/name).exists()for name in ('dri','snd','video0','tty'))
out=a.output;env=dict(os.environ,FONTCONFIG_FILE=str(a.font_config),LANG='C.UTF-8',LC_ALL='C.UTF-8',WLR_BACKENDS='headless',WLR_HEADLESS_OUTPUTS='1',WLR_RENDERER='gles2'if a.effects else 'pixman',WLR_RENDERER_FORCE_SOFTWARE='1'if a.effects else '0',WLR_RENDERER_ALLOW_SOFTWARE='1',WLR_LIBINPUT_NO_DEVICES='1',LIBGL_ALWAYS_SOFTWARE='1',MESA_LOADER_DRIVER_OVERRIDE='llvmpipe',LIBGL_DRIVERS_PATH=str(a.software_driver/'lib/dri'),__EGL_VENDOR_LIBRARY_FILENAMES=str(a.software_driver/'share/glvnd/egl_vendor.d/50_mesa.json'),QT_QPA_PLATFORM='wayland',QT_QUICK_BACKEND='software',QT_QPA_PLATFORMTHEME='generic',QT_ACCESSIBILITY='0',NO_AT_BRIDGE='1',GTK_A11Y='none',LUMEN_UNIFIED_OWNED_HEADERS='1'if a.frame=='unified'else'0',LUMEN_NATIVE_WINDOW_CONTROLS='0'if a.frame=='unified'else'1',LUMEN_NATIVE_WINDOW_ACTIONS='0',LUMEN_FILE_SERVICE='0',LUMEN_LAUNCH_SCOPES='0',LUMEN_NEARBY_SHARING_PROVIDER='',GLOBAL_MENU_PYTHON=sys.executable)
for key in ('DISPLAY','WAYLAND_DISPLAY','SWAYSOCK'):env.pop(key,None)
assets=out/'desktop';shutil.copytree(repo/'res/lumen-desktop/shell',assets);backend=assets/'files.py';original=backend.read_text();needle="    if action=='volumes':\n";assert original.count(needle)==1;backend.write_text(original.replace(needle,needle+'        return dict(volumes=[])\n'));(out/'provider-seams.json').write_text(json.dumps(dict(path='files.py',before=hashlib.sha256(original.encode()).hexdigest(),after=hashlib.sha256(backend.read_bytes()).hexdigest(),scope='Volume discovery excluded; actual disposable directory listing retained')))
folder=Path('/home/fixture/Documents');folder.mkdir();(folder/'Private document.txt').write_text('Unchanged private fixture\n');palettes=json.loads((repo/'res/lumen-desktop/appearance/palette.json').read_text());appearance=out/'appearance.json';env['LUMEN_APPEARANCE_STATE']=str(appearance)
def theme(mode,opaque):
 pending=appearance.with_suffix('.pending');pending.write_text(json.dumps(dict(font=palettes['font'],palette=palettes[mode],preferences=dict(reduceMotion=True,reduceTransparency=opaque))));pending.replace(appearance)
theme('light',False)
qmls={}
for kind in ('Files','Settings','Calendar'):
 qml=out/(kind+'.qml');qmls[kind]=qml;qml.write_text('''import QtQuick
import QtQuick.Controls
import QtQuick.Window
import Quickshell
import Quickshell.Io
import "desktop" as D
ShellRoot {
 id:audit
 property string kind:'''+json.dumps(kind)+'''
 D.OwnedWindowContext {id:context}
 QtObject {id:exported;property var state:({appName:"",title:"",windowId:0})}
 QtObject {id:stub;property var ownedContext:context;property date currentDate:new Date(2026,9,20);property int focusEpoch:0;property var windows:[];property var exportedMenu:exported;property string volume:"Volume: 0.5";property var audioState:({available:false,muted:false,outputs:[],inputs:[],streams:[],inputAvailable:false,sink:null,source:null});property var systemState:({});property var search:({available:false,scanning:false,indexed:0});property var notifications:({dnd:false,available:false,applications:[],policies:{},history:[],error:""});function activeFiles(window){}function focusWindow(window){}function claimPopup(name){}function focusOwnedWindow(name){}function openSettings(name){}}
 D.FilesState {id:service;persistViewPreferences:false;path:"/home/fixture/Documents";viewMode:"list"}
 D.SettingsState {id:settingsService;active:false;available:true;system:({system:"Private fixture",desktop:"Private fixture"})}
 D.FilesWindow {id:files;shell:stub;service:service}
 D.SettingsWindow {id:settings;shell:stub;service:settingsService;page:"Appearance"}
 D.CalendarWindow {id:calendar;shell:stub}
 property var main:kind==="Files"?files:kind==="Settings"?settings:calendar
 Component.onCompleted:{calendar.calendarState.active=false;calendar.calendarState.loaded=true;service.navigate(service.path);main.visible=true;}
 function walk(item,seen){if(!item || seen.includes(item))return [];seen.push(item);let found=[item];for(const child of Array.from(item.children || []).concat(Array.from(item.resources || [])))found=found.concat(walk(child,seen));if(item.contentItem)found=found.concat(walk(item.contentItem,seen));return found;}
 function field(window){return walk(window.contentItem,[]).find(item=>item.placeholderText===(kind==="Files"?"Filter folder or tag:red":kind==="Settings"?"Search":"Search"));}
 function rect(item,w){const point=item.mapToItem(w.contentItem,0,0);return {x:point.x,y:point.y,width:item.width,height:item.height};}
 IpcHandler {target:"ownedInactive"
  function ready():string {return service.ready && !service.busy?"ready":"pending";}
  function selectDocument():void {files.choose(0,0);}
  function openUtility():void {files.informationWindow.show({name:"Private document.txt",size:"26 bytes",kind:"Text",location:"Private synthetic directory",permissions:"Read and write"});}
  function focusControl(utility:bool):void {const w=utility?files.informationWindow:main,target=utility?walk(w.contentItem,[]).find(item=>item.Accessible.name==="General" && item.background):field(w);if(target)target.forceActiveFocus();}
  function state(utility:bool):string {const w=utility?files.informationWindow:main,native=w.contentItem.Window.window,f=native?native.activeFocusItem:null,items=walk(w.contentItem,[]),controls=items.find(item=>item.controlsHovered!==undefined && item.windowName!==undefined),buttons=controls?walk(controls,[]).filter(item=>item.index!==undefined && item.background && item.modelData!==undefined):[],selected=utility?null:kind==="Files"?items.find(item=>item.text==="Private document.txt"):kind==="Settings"?items.find(item=>item.objectName==="settings-navigation-Appearance"):null,selectedItem=selected?(kind==="Files"?selected.parent:selected.background):null,primary=items.find(item=>item.fontInfo && item.text===(utility?"Private document.txt":kind==="Files"?"Private document.txt":kind==="Settings"?"Appearance":Qt.formatDate(calendar.month,"MMMM yyyy")));return JSON.stringify({kind:audit.kind,utility:utility,title:w.title,visible:w.visible,active:!!native && native.active,width:w.width,height:w.height,dark:D.Theme.dark,opaque:!!D.Theme.preferences.reduceTransparency,selectionActive:utility?null:kind==="Files"?files.selectionActive:kind==="Settings"?settings.selectionActive:null,selectionFill:utility?null:kind==="Files"?String(files.selectionFill):kind==="Settings"?String(settings.selectionFill):null,selectedPaths:service.rows.length?files.selectedPaths:[],selectedPage:settings.page,selectionRect:selectedItem?rect(selectedItem,w):null,primary:primary?{rect:rect(primary,w),color:String(primary.color)}:null,body:String(D.Theme.colors.background),text:String(D.Theme.colors.text),controls:buttons.map(item=>({index:item.index,enabled:item.enabled,color:String(item.background.color),rect:rect(item,w)})),focus:f?{name:f.Accessible.name || f.placeholderText || "",rect:rect(f,w),border:f.background?String(f.background.border.color):null}:null,context:{keyMain:context.keyWindow===files,keyUtility:context.keyWindow===files.informationWindow,mainFiles:context.mainWindow===files,application:context.applicationId,role:context.keyRole}});}
 }
}''')
config=out/'sway.conf';config.write_text('include /source/res/lumen-desktop/sway.conf\noutput HEADLESS-1 resolution 2000x900\noutput * bg #29445b solid_color\nfocus_follows_mouse no\n'+('for_window [app_id="org.quickshell"] border normal 1\n'if a.frame=='native'else''))
processes=[];keyboard=None;records=[];clients={}
def launch(command,name):
 with (out/name).open('w')as log:process=sp.Popen(list(map(str,command)),env=env,stdout=log,stderr=sp.STDOUT,start_new_session=True)
 processes.append(process);return process
def call(*command):return sp.check_output(list(map(str,command)),env=env,text=True,timeout=5).strip()
def nodes(node):
 yield node
 for child in node.get('nodes',[])+node.get('floating_nodes',[]):yield from nodes(child)
def tree():return json.loads(call(a.sway.parent/'swaymsg','-r','-t','get_tree'))
def ipc(kind,method,*args):return call('quickshell','ipc','-p',qmls[kind],'call','ownedInactive',method,*args)
def state(kind,utility=False):return json.loads(ipc(kind,'state','true'if utility else'false'))
def wait(predicate,label):
 deadline=time.monotonic()+15
 while time.monotonic()<deadline:
  try:
   value=predicate()
   if value:return value
  except (OSError,ValueError,sp.SubprocessError,StopIteration):pass
  time.sleep(.03)
 (out/'failure.json').write_text(json.dumps(dict(label=label,records=records,tree=tree()),indent=2));raise AssertionError(label)
def command(value):
 result=json.loads(call(a.sway.parent/'swaymsg','-r',value));assert all(row.get('success')for row in result),result
try:
 start_private_bus(env,out,processes);launch([a.sway,'-c',config],'sway.log');runtime=Path(env['XDG_RUNTIME_DIR']);env['SWAYSOCK']=str(wait(lambda:next(runtime.glob('sway-ipc*.sock'),None),'private compositor'));env['WAYLAND_DISPLAY']=wait(lambda:next((path.name for path in runtime.glob('wayland-*')if not path.name.endswith('.lock')),None),'private Wayland display');
 with (out/'keyboard.log').open('w')as log:keyboard=PhysicalKeyboard(a.keyboard,env,log)
 processes.append(keyboard.process)
 for index,kind in enumerate(('Files','Settings','Calendar')):
  clients[kind]=launch(['quickshell','--no-duplicate','-p',qmls[kind]],kind+'.log');wait(lambda:ipc(kind,'ready')=='ready',kind+' private listing');node=wait(lambda:next((row for row in nodes(tree())if row.get('pid')==clients[kind].pid and row.get('name')==state(kind)['title']),None),'exact independent client');command('[con_id='+str(node['id'])+'] resize set width 480 px height 650 px, move position '+str(index*500+10)+' px 100 px')
 ipc('Files','selectDocument');wait(lambda:state('Files')['selectedPaths']==[str(folder/'Private document.txt')],'captured private selected document');ipc('Files','openUtility');utility=wait(lambda:next((row for row in nodes(tree())if row.get('pid')==clients['Files'].pid and row.get('name')=='File Info'),None),'exact captured utility');command('[con_id='+str(utility['id'])+'] move position 1520 px 100 px')
 assert len({process.pid for process in clients.values()})==3
 for mode,opaque in (('light',False),('dark',False),('light',True),('dark',True)):
  theme(mode,opaque);wait(lambda:all(state(kind)['dark']==(mode=='dark')and state(kind)['opaque']==opaque for kind in clients),'all independent live palettes')
  for kind,is_utility in (('Files',False),('Settings',False),('Calendar',False),('Files',True)):
   target=wait(lambda:next((row for row in nodes(tree())if row.get('pid')==clients[kind].pid and row.get('name')==state(kind,is_utility)['title']),None),'captured focus target');command('[con_id='+str(target['id'])+'] focus');wait(lambda:state(kind,is_utility)['active'],'actual key recipient');snapshots=[state(name)for name in clients]+[state('Files',True)];assert sum(row['active']for row in snapshots)==1,snapshots
   for row in snapshots:
    if row['selectionActive']is not None:assert row['selectionActive']==row['active'],row
    assert row['selectedPaths']==[str(folder/'Private document.txt')]if row['kind']=='Files'else True,row
    assert row['selectedPage']=='Appearance',row
    if a.frame=='unified':
     for button in row['controls']:
      if button['enabled']:assert button['color']in(['#ff5f57','#febc2e','#28c840']if row['active']else['#555559'if mode=='dark'else'#ceced2']),button
   if is_utility:assert snapshots[-1]['context']==dict(keyMain=False,keyUtility=True,mainFiles=True,application='lumen-files',role='utility'),snapshots[-1]
   ipc(kind,'focusControl','true'if is_utility else'false');wait(lambda:state(kind,is_utility)['focus']and state(kind,is_utility)['focus']['name'],'actual production focused control');before_focus=state(kind,is_utility)['focus'];keyboard.key('-k','Tab');wait(lambda:state(kind,is_utility)['active'] and state(kind,is_utility)['focus'] and state(kind,is_utility)['focus']['name']!=before_focus['name'],'physicalTab reaches distinct control on exact recipient');after_focus=state(kind,is_utility)['focus'];capture=out/(mode+('-opaque'if opaque else'-material')+'-'+kind+('-utility'if is_utility else'')+'.png');
   painted=[]
   def painted_receipt():
    from PIL import Image,ImageColor
    call('grim','-o','HEADLESS-1',capture);current=list(nodes(tree()));checks=[]
    with Image.open(capture)as frame:
     frame=frame.convert('RGB')
     for row in snapshots:
      node=next(n for n in current if n.get('pid')==clients[row['kind']].pid and n.get('name')==row['title']);origin=(node['rect']['x']+node['window_rect']['x'],node['rect']['y']+node['window_rect']['y'])
      if a.frame=='unified':
       for button in row['controls']:
        if not button['enabled']:continue
        rect=button['rect'];point=(round(origin[0]+rect['x']+2),round(origin[1]+rect['y']+rect['height']/2));checks.append(dict(kind=row['kind'],utility=row['utility'],surface='owned-control',point=point,expected=ImageColor.getrgb(button['color']),actual=frame.getpixel(point)))
      else:
       deco=node['deco_rect'];styles={parts[0]:parts for parts in (line.split()for line in (repo/'res/lumen-desktop/sway.conf').read_text().splitlines())if parts and parts[0]in('client.focused','client.unfocused')};style=styles['client.focused'if row['active']else'client.unfocused'];foreground=ImageColor.getrgb(style[3]);box=(deco['x']+80,deco['y']+3,deco['x']+deco['width']-10,deco['y']+deco['height']-3);ink=sum(max(abs(a-b)for a,b in zip(pixel,foreground))<=2 for pixel in frame.crop(box).get_flattened_data());checks.append(dict(kind=row['kind'],utility=row['utility'],surface='native-title-foreground',matchingPixels=ink,minimumPixels=4,expected=foreground))
       point=(deco['x']+20,deco['y']+16);checks.append(dict(kind=row['kind'],utility=row['utility'],surface='native-close',point=point,expected=(255,95,87)if row['active']else(140,140,145),actual=frame.getpixel(point)))
      assert row['primary'],row
      rect=row['primary']['rect'];foreground=ImageColor.getrgb(row['primary']['color']);box=(max(0,round(origin[0]+rect['x'])),max(0,round(origin[1]+rect['y'])),min(frame.width,round(origin[0]+rect['x']+rect['width'])),min(frame.height,round(origin[1]+rect['y']+rect['height'])));ink=sum(max(abs(a-b)for a,b in zip(pixel,foreground))<=2 for pixel in frame.crop(box).get_flattened_data());checks.append(dict(kind=row['kind'],utility=row['utility'],surface='readable-primary-text',matchingPixels=ink,minimumPixels=4,expected=foreground))
      if row['selectionRect']:
       rect=row['selectionRect'];point=(round(origin[0]+rect['x']+rect['width']-8),round(origin[1]+rect['y']+rect['height']/2));checks.append(dict(kind=row['kind'],utility=row['utility'],surface='selection',point=point,expected=ImageColor.getrgb(row['selectionFill']),actual=frame.getpixel(point)))
    painted[:]=checks
    (out/'last-paint-receipt.json').write_text(json.dumps(checks,indent=2))
    return all(row['matchingPixels']>=row['minimumPixels'] if 'matchingPixels'in row else max(abs(a-b)for a,b in zip(row['expected'],row['actual']))<=2 for row in checks)
   wait(painted_receipt,'actual painted ownership receipt');records.append(dict(mode=mode,opaque=opaque,target=dict(kind=kind,utility=is_utility,pid=clients[kind].pid,conId=target['id']),states=snapshots,focusBefore=before_focus,focusAfter=after_focus,painted=painted,capture=str(capture)))
 assert (folder/'Private document.txt').read_text()=='Unchanged private fixture\n'
 for kind in clients:assert not re.search(r'ReferenceError|TypeError|binding loop',(out/(kind+'.log')).read_text()),kind
 (out/'result.json').write_text(json.dumps(dict(passed=True,frame=a.frame,effects=a.effects,cases=records,independentClients=3,utilityOwner='Files',capabilities=dict(independentOwnedClients=True,utilityContextAndSelection=True,liveInactiveSelections=True,paintedUnifiedControls=a.frame=='unified',paintedNativeControls=a.frame=='native',physicalTabRecipient=True,allShellSurfaces=False,externalApplications=False,referenceMeasurements=False),scope='Actual independent owned-client key selection/control ownership and physicalTab; no external application or reference-exact claim'),indent=2))
finally:
 if keyboard:keyboard.close()
 for process in reversed(processes):
  if process.poll()is None:
   os.killpg(process.pid,signal.SIGTERM)
   try:process.wait(timeout=5)
   except sp.TimeoutExpired:os.killpg(process.pid,signal.SIGKILL);process.wait(timeout=5)
 close_reaped_pipes(processes)

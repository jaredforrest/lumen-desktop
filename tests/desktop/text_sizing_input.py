"""Actual enlarged Settings navigation, focus reveal and pointer activation."""
import argparse,json,os,re,shutil,signal,subprocess as sp,sys,tempfile,time
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--output',type=Path,required=True);p.add_argument('--launcher',type=Path,required=True);p.add_argument('--sway',required=True);p.add_argument('--pointer-tool',required=True);p.add_argument('--inside',action='store_true');a=p.parse_args()
if not a.inside:
    with tempfile.TemporaryDirectory(prefix='lumen-text-input-',ignore_cleanup_errors=True) as private:
        env=dict(os.environ,HOME=private,XDG_RUNTIME_DIR=private)
        for key in ('CONFIG','STATE','CACHE','DATA'):env['XDG_'+key+'_HOME']=str(Path(private)/key.lower())
        for key in ('DISPLAY','WAYLAND_DISPLAY','SWAYSOCK','DBUS_SESSION_BUS_ADDRESS','AT_SPI_BUS_ADDRESS'):env.pop(key,None)
        config=Path(private)/'bus.conf'
        config.write_text('<busconfig><type>session</type><listen>unix:tmpdir='+private+'</listen><auth>EXTERNAL</auth><policy context="default"><allow send_destination="*"/><allow receive_sender="*"/><allow own="*"/></policy></busconfig>')
        raise SystemExit(sp.call(['dbus-run-session','--config-file='+str(config),'--',sys.executable,__file__,*sys.argv[1:],'--inside'],env=env))
repo=Path(__file__).resolve().parents[2];out=a.output.resolve();out.mkdir(parents=True,exist_ok=False)
prefix=re.split(r'\n[ \t]*exec quickshell ',a.launcher.read_text(),maxsplit=1)[0]
exported=sp.check_output(['bash','-c',prefix+'\nexec env -0']);env=dict(value.decode().split('=',1) for value in exported.split(b'\0') if b'=' in value)
env.update(WLR_BACKENDS='headless',WLR_RENDERER='pixman',WLR_LIBINPUT_NO_DEVICES='1',QT_QPA_PLATFORM='wayland',QT_QPA_PLATFORMTHEME='generic',QT_QUICK_BACKEND='software',QT_ACCESSIBILITY='0',QT_LINUX_ACCESSIBILITY_ALWAYS_ON='0',NO_AT_BRIDGE='1',GTK_A11Y='none',LUMEN_FILE_SERVICE='0',LUMEN_LAUNCH_SCOPES='0',LUMEN_NATIVE_WINDOW_CONTROLS='0',LUMEN_NATIVE_WINDOW_ACTIONS='0',LUMEN_UNIFIED_OWNED_HEADERS='1')
for key in ('DISPLAY','WAYLAND_DISPLAY','SWAYSOCK','AT_SPI_BUS_ADDRESS'):env.pop(key,None)
palettes=json.loads((repo/'res/lumen-desktop/appearance/palette.json').read_text());appearance=out/'appearance.json';appearance.write_text(json.dumps(dict(font=palettes['font'],palette=palettes['dark'],preferences=dict(textScale=1.5))));env['LUMEN_APPEARANCE_STATE']=str(appearance);env['GLOBAL_MENU_PYTHON']=sys.executable
shutil.copytree(repo/'res/lumen-desktop',out/'assets')
qml=out/'shell.qml';qml.write_text('''import QtQuick
import QtQuick.Window
import Quickshell
import Quickshell.Io
import "assets/shell" as D
ShellRoot {
 id:fixture
 QtObject {id:service
  property var menuBarPreferences:D.Theme.menuBarPreferences
  property var preferences:D.Theme.preferences
  property var desktopPreferences:D.Theme.desktopPreferences
  property var windowsPreferences:D.Theme.windowsPreferences
  property var notificationsPreferences:({preview:"always"})
  property var searchPreferences:({disabledProviders:[]})
  property var appearance:({})
  property var system:({system:"Fixture",desktop:"Fixture"})
  property bool available:true
  property bool busy:false
  property bool applying:false
  property string error:""
  property string warning:""
  function refresh(){}
  function setPreference(key,value){const next=Object.assign({},preferences);next[key]=value;preferences=next;D.Theme.preferences=next;}
  function setMenuBarPreference(key,value){}
  function setDesktopPreference(key,value){}
  function setWindowPreference(key,value){}
  function setNotificationPreference(key,value){}
  function setSearchPreference(key,value){}
 }
 QtObject {id:stub
  property string volume:"Volume: 0.5"
  property var audioState:({available:false,muted:false,outputs:[],inputs:[],streams:[],inputAvailable:false,sink:null,source:null})
  property var notifications:({dnd:false,available:false,applications:[],policies:{},history:[],error:""})
  property var search:({available:false,scanning:false,indexed:0})
  property var systemState:({})
  property var vpn:({})
 }
 D.SettingsWindow {id:settings;service:service;shell:stub;visible:true;page:"Accessibility";implicitWidth:480;implicitHeight:320}
 function find(name,item){if(item.objectName===name)return item;for(const child of item.children || []){const result=find(name,child);if(result)return result;}return null;}
 function rectangle(item){const at=item.mapToItem(settings.contentItem,0,0);return {x:at.x,y:at.y,width:item.width,height:item.height};}
 IpcHandler {target:"filesAudit"
  function state():string {const scroll=fixture.find("settings-content-scroll",settings.contentItem);const focus=settings.contentItem.Window.window.activeFocusItem;return JSON.stringify({ready:settings.visible,scale:D.Theme.textScale,width:settings.width,height:settings.height,motion:!!service.preferences.reduceMotion,focus:focus?focus.objectName:"",focused:focus?fixture.rectangle(focus):null,viewport:fixture.rectangle(scroll),scrollY:scroll.contentY,contentHeight:scroll.contentHeight});}
  function navigate(name:string):void {settings.navigate(name);}
  function focus(name:string):void {fixture.find(name,settings.contentItem).forceActiveFocus();}
  function geometry(name:string):string {const item=fixture.find(name,settings.contentItem);return item?JSON.stringify(fixture.rectangle(item)):"null";}
 }
}''')
processes=[]
def launch(command,name):
    with (out/name).open('w') as log:process=sp.Popen(command,env=env,stdout=log,stderr=sp.STDOUT,start_new_session=True)
    processes.append(process);return process

def call(*command):return sp.check_output(command,env=env,text=True,timeout=10).strip()
def wait(fn):
    end=time.monotonic()+20
    while time.monotonic()<end:
        try:
            value=fn()
            if value:return value
        except (OSError,ValueError,sp.SubprocessError):pass
        time.sleep(.1)
    if (out/'shell.log').exists():
        try:(out/'failed-state.json').write_text(json.dumps(state(),indent=2))
        except Exception:pass
    raise AssertionError('Files input fixture timed out; inspect logs')
def nodes(node):
    yield node
    for child in node.get('nodes',[])+node.get('floating_nodes',[]):yield from nodes(child)
def window():return next(node for node in nodes(json.loads(call('swaymsg','-r','-t','get_tree'))) if node.get('app_id')=='org.quickshell' and node.get('name','')=='System Settings')
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
    launch(['quickshell','-p',str(qml)],'shell.log');wait(lambda:state()['ready'] and state()['scale']==1.5)
    launch(['wtype','-s','60000'],'keyboard.log')
    pointer=sp.Popen([a.pointer_tool,'pointer','move','0','0'],env=env,stdin=sp.PIPE,text=True,start_new_session=True);processes.append(pointer);time.sleep(.2)
    node=window();call('swaymsg','[con_id='+str(node['id'])+'] resize set width 480 px height 320 px');wait(lambda:state()['width']==480 and state()['height']==320)
    checks=[]
    ipc('focus','settings-text-size');time.sleep(.1)
    for expected in ('settings-increase-contrast','', 'settings-reduce-motion'):
        key('-k','Tab')
        if expected:wait(lambda:state()['focus']==expected)
        value=state();focus=value['focused'];viewport=value['viewport'];assert focus['y']>=viewport['y']-1 and focus['y']+focus['height']<=viewport['y']+viewport['height']+1,value
    assert state()['scrollY']>0,state()
    checks.append('Actual Tab navigation reveals enlarged below-fold controls in a480x320 Settings window')
    click(point('settings-reduce-motion'));wait(lambda:state()['motion'])
    checks.append('Revealed enlarged control remains clickable and updates its preference')
    key('-M','shift','-k','Tab','-m','shift');value=state();focus=value['focused'];viewport=value['viewport'];assert focus['y']>=viewport['y']-1 and focus['y']+focus['height']<=viewport['y']+viewport['height']+1,value
    checks.append('Reverse keyboard navigation scrolls the previous control fully into view')
    ipc('focus','settings-reduce-motion');time.sleep(.1)
    for scale in (1,1.5):
        pending=appearance.with_suffix('.new');pending.write_text(json.dumps(dict(font=palettes['font'],palette=palettes['dark'],preferences=dict(textScale=scale))));pending.replace(appearance)
        wait(lambda:state()['scale']==scale);time.sleep(.1)
        value=state();focus=value['focused'];viewport=value['viewport'];assert value['focus']=='settings-reduce-motion' and focus['y']>=viewport['y']-1 and focus['y']+focus['height']<=viewport['y']+viewport['height']+1,value
    checks.append('Live default/enlarged text changes preserve focus and reveal the resized control')
    call('grim',str(out/'settings-enlarged-dark.png'))
    ipc('focus','settings-navigation-Search');time.sleep(.15);value=state();assert value['focused']['y']>=100 and value['focused']['y']+value['focused']['height']<=320,value
    checks.append('Sidebar keyboard focus reveals its last page without activating another pane')
    ipc('navigate','Appearance');ipc('focus','settings-accent-green');time.sleep(.15)
    accent=json.loads(ipc('geometry','settings-accent-green'));assert accent['x']+accent['width']<=state()['width']-24,accent
    click(point('settings-accent-green'))
    ipc('focus','settings-icon-choice-auto');time.sleep(.15)
    choices=json.loads(ipc('geometry','settings-icon-style-choices'));description=json.loads(ipc('geometry','settings-icon-style-description'));card=json.loads(ipc('geometry','settings-icon-style-card'))
    assert description['y']>=choices['y']+choices['height']+9 and description['y']+description['height']<=card['y']+card['height']-13,(choices,description,card)
    key('-k','Tab');wait(lambda:state()['focus']=='settings-icon-choice-dark');value=state();focus=value['focused'];viewport=value['viewport'];assert focus['y']>=viewport['y']-1 and focus['y']+focus['height']<=viewport['y']+viewport['height']+1,value
    call('grim',str(out/'settings-appearance-enlarged.png'))
    checks.append('Enlarged Appearance options grow without overlapping explanatory text and remain keyboard reachable')
    ipc('navigate','Accessibility');ipc('focus','settings-reduce-motion');time.sleep(.1)
    pending=appearance.with_suffix('.new');pending.write_text(json.dumps(dict(font=palettes['font'],palette=palettes['light'],preferences=dict(textScale=1.5))));pending.replace(appearance);time.sleep(.3)
    call('grim',str(out/'settings-enlarged-light.png'))
    from PIL import Image,ImageStat
    image=Image.open(out/'settings-enlarged-dark.png').convert('RGB');r=window()['rect'];crop=image.crop((r['x']+200,r['y']+70,r['x']+r['width']-15,r['y']+r['height']-15));assert max(ImageStat.Stat(crop).mean)>10 and max(ImageStat.Stat(crop).stddev)>2,'Blank Settings interior'
    log=(out/'shell.log').read_text();assert not any(marker in log for marker in ('ReferenceError:','TypeError:','Binding loop','Failed to load configuration','Cannot assign')),log
    (out/'result.json').write_text(json.dumps(dict(passed=True,checks=checks,final=state(),scope='Actual owned Settings with disposable preference backend, genuine nested keyboard and pointer; no live changes'),indent=2)+'\n')
finally:
    for process in reversed(processes):
        if process.poll() is None:os.killpg(process.pid,signal.SIGTERM)
    for process in reversed(processes):
        try:process.wait(timeout=4)
        except sp.TimeoutExpired:os.killpg(process.pid,signal.SIGKILL);process.wait(timeout=4)
    if 'pointer' in globals() and pointer.stdin:pointer.stdin.close()
print('PASS native enlarged Settings focus, scroll and pointer reachability')

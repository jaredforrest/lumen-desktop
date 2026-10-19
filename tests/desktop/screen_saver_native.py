"""Decorative saver and host-lock separation on a private compositor."""
import argparse,json,os,re,shutil,signal,subprocess as sp,sys,tempfile,time
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--output',type=Path,required=True);p.add_argument('--launcher',type=Path,required=True);p.add_argument('--sway',required=True);p.add_argument('--pointer-tool',required=True);p.add_argument('--lock',required=True);p.add_argument('--effects',action='store_true');p.add_argument('--inside',action='store_true');a=p.parse_args()
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
palettes=json.loads((repo/'res/lumen-desktop/appearance/palette.json').read_text());appearance=out/'appearance.json';appearance.write_text(json.dumps(dict(font=palettes['font'],palette=palettes['dark'],preferences={})));env['LUMEN_APPEARANCE_STATE']=str(appearance);env['GLOBAL_MENU_PYTHON']=sys.executable
shutil.copytree(repo/'res/lumen-desktop',out/'assets')
env.update(settings_adapter(out,env['PATH']))
from PIL import Image,ImageStat
qml=out/'shell.qml';qml.write_text('''import QtQuick
import QtQuick.Controls
import QtQuick.Window
import Quickshell
import Quickshell.Io
import Quickshell.Wayland
import "assets/shell" as D
ShellRoot {
 id:root
 property bool inhibit:false
 property int underlyingClicks:0
 D.ScreenSaverState {id:service;active:true}
 D.ScreenSaverController {id:saver;state:service}
 FloatingWindow {id:window;title:"Saver fixture";visible:true;implicitWidth:460;implicitHeight:600;color:D.Theme.colors.background
  Column {anchors.fill:parent;anchors.margins:16;spacing:12
   TextField {id:underlying;objectName:"underlying-input";width:parent.width;placeholderText:"Underlying application input"}
   D.DialogButton {objectName:"underlying-button";text:"Underlying action";onClicked:++root.underlyingClicks}
   D.ScreenSaverPane {width:parent.width;service:service;onPreviewRequested:saver.start(true)}
  }
 }
 IdleInhibitor {window:window;enabled:root.inhibit}
 function find(name,item){if(!item)return null;if(item.objectName===name)return item;for(const child of item.children || []){const result=find(name,child);if(result)return result;}return null;}
 IpcHandler {target:"filesAudit"
  function refresh():void {service.refresh();}
  function preview():void {saver.start(true);}
  function inhibit(value:bool):void {root.inhibit=value;}
  function preference(key:string,value:string):void {service.setValue(key,JSON.parse(value));}
  function focusInput():void {underlying.forceActiveFocus();}
  function state():string {return JSON.stringify({ready:!service.busy,preferences:service.preferences,permitted:service.permitted,lock:service.lock,showing:saver.showing,reason:saver.reason,text:underlying.text,clicks:root.underlyingClicks,error:service.error});}
  function geometry(name:string):string {const item=find(name,window.contentItem);if(!item)return "null";const at=item.mapToItem(window.contentItem,0,0);return JSON.stringify({x:at.x,y:at.y,width:item.width,height:item.height});}
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
def window():return next(node for node in nodes(json.loads(call('swaymsg','-r','-t','get_tree'))) if node.get('app_id')=='org.quickshell' and node.get('name','')=='Saver fixture')
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
    launch(['quickshell','-p',str(qml)],'shell.log');wait(lambda:window());wait(lambda:state()['permitted'] and state()['ready'])
    checks=[];initial=state();assert initial['preferences']['enabled'] is False and initial['showing'] is False
    click(point('screen-saver-preview'));wait(lambda:state()['showing']);time.sleep(.2);key('-k','Escape');wait(lambda:not state()['showing']);assert state()['preferences']['enabled'] is False
    checks.append('Actual owned Preview button starts and cancels decoration without enabling automatic idle activation')
    ipc('focusInput');ipc('preview');wait(lambda:state()['showing']);time.sleep(.5)
    def painted():
        call('grim',str(out/'saver-dark.png'))
        image=Image.open(out/'saver-dark.png').convert('RGB')
        return max(ImageStat.Stat(image.crop((400,780,880,860))).stddev)>5
    wait(painted)
    image=Image.open(out/'saver-dark.png').convert('RGB');assert min(image.getpixel((640,450)))>=10 and max(ImageStat.Stat(image).stddev)>5,'Unpainted saver'
    key('x');wait(lambda:not state()['showing']);assert state()['text']=='',state();key('y');assert state()['text']=='y',state()
    checks.append('Default-off decorative preview paints; first key is consumed, next key reaches restored application focus')
    position=point('underlying-button');move(*position);ipc('preview');wait(lambda:state()['showing']);time.sleep(.3);command('p');command('r');wait(lambda:not state()['showing']);assert state()['clicks']==0;click(position);assert state()['clicks']==1
    checks.append('Dismissing pointer press/release does not trigger underlying application action')
    ipc('preview');wait(lambda:state()['showing']);time.sleep(.3);move(200,200);command('m 5 5');wait(lambda:not state()['showing'])
    checks.append('Actual pointer motion dismisses decorative overlay')
    ipc('inhibit','true');ipc('preference','idleSeconds','15');wait(lambda:state()['preferences']['idleSeconds']==15 and state()['ready']);ipc('preference','enabled','true');wait(lambda:state()['preferences']['enabled'] and state()['ready']);time.sleep(16);assert not state()['showing'],state()
    checks.append('Mapped real idle inhibitor prevents automatic activation beyond configured delay')
    ipc('inhibit','false');wait(lambda:state()['showing']);key('-k','Escape');wait(lambda:not state()['showing'])
    checks.append('Actual idle-notify activation after inhibitor removal dismisses on input')
    ipc('preference','enabled','false');wait(lambda:not state()['preferences']['enabled'] and state()['ready'])
    ipc('preview');wait(lambda:state()['showing'])
    with (out/'lock.log').open('w') as log:locker=sp.Popen([a.lock],env=env,stdin=sp.PIPE,stdout=log,stderr=sp.STDOUT,text=True,start_new_session=True)
    processes.append(locker);wait(lambda:state()['lock']['known'] and state()['lock']['locked'] and not state()['showing'])
    ipc('preview');key('z');assert state()['lock']['locked'] and not state()['showing'],state()
    checks.append('Real session-lock protocol takes precedence; saver dismissal/start cannot unlock or bypass authentication')
    locker.stdin.write('u\n');locker.stdin.flush();locker.wait(timeout=5);locker.stdin.close();wait(lambda:state()['permitted']);assert not state()['showing'];ipc('preview');wait(lambda:state()['showing']);key('-k','Escape');wait(lambda:not state()['showing'])
    checks.append('After protocol-authorized unlock decoration can be previewed again independently of disabled automatic saver')
    assert json.loads((out/'settings.json').read_text())['screenSaver']==dict(enabled=False,idleSeconds=15,motion=True)
    log=(out/'shell.log').read_text();assert not any(marker in log for marker in ('ReferenceError:','TypeError:','Binding loop','Failed to load configuration','Cannot assign')),log
    (out/'result.json').write_text(json.dumps(dict(passed=True,checks=checks,scope='Private compositor, real idle inhibitor/input/session-lock protocol; no host policy, credentials or live settings'),indent=2)+'\n')
finally:
    for process in reversed(processes):
        if process.poll() is None:os.killpg(process.pid,signal.SIGTERM)
    if 'pointer' in globals() and pointer.stdin:pointer.stdin.close()
    for process in reversed(processes):
        try:process.wait(timeout=4)
        except sp.TimeoutExpired:os.killpg(process.pid,signal.SIGKILL)
print('PASS native decorative saver, inhibitors, consumed dismissal and secure lock precedence')

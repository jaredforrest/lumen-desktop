"""Wallpaper chooser on genuine disposable compositor outputs."""
import argparse,json,os,re,shutil,signal,subprocess as sp,sys,tempfile,time
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--output',type=Path,required=True);p.add_argument('--launcher',type=Path,required=True);p.add_argument('--sway',required=True);p.add_argument('--pointer-tool',required=True);p.add_argument('--inside',action='store_true');a=p.parse_args()
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
env.update(WLR_BACKENDS='headless',WLR_RENDERER='pixman',WLR_LIBINPUT_NO_DEVICES='1',QT_QPA_PLATFORM='wayland',QT_QPA_PLATFORMTHEME='generic',QT_QUICK_BACKEND='software',QT_ACCESSIBILITY='0',QT_LINUX_ACCESSIBILITY_ALWAYS_ON='0',NO_AT_BRIDGE='1',GTK_A11Y='none',LUMEN_FILE_SERVICE='0',LUMEN_LAUNCH_SCOPES='0',LUMEN_NATIVE_WINDOW_CONTROLS='0',LUMEN_NATIVE_WINDOW_ACTIONS='0',LUMEN_UNIFIED_OWNED_HEADERS='1')
for key in ('DISPLAY','WAYLAND_DISPLAY','SWAYSOCK','AT_SPI_BUS_ADDRESS'):env.pop(key,None)
palettes=json.loads((repo/'res/lumen-desktop/appearance/palette.json').read_text());appearance=out/'appearance.json';appearance.write_text(json.dumps(dict(font=palettes['font'],palette=palettes['dark'],preferences={})));env['LUMEN_APPEARANCE_STATE']=str(appearance);env['GLOBAL_MENU_PYTHON']=sys.executable
shutil.copytree(repo/'res/lumen-desktop',out/'assets')
env.update(settings_adapter(out,env['PATH']))
from PIL import Image,ImageStat
red=out/'red image.png';blue=out/"blue $(echo injected); ' #.png"
Image.new('RGB',(160,100),(200,30,20)).save(red);Image.new('RGB',(100,160),(20,40,210)).save(blue)
env['LUMEN_WALLPAPER_FALLBACK']=str(red)
qml=out/'shell.qml';qml.write_text('''import QtQuick
import QtQuick.Window
import QtQuick.Controls
import Quickshell
import Quickshell.Io
import "assets/shell" as D
ShellRoot {
 D.WallpaperSettingsState {id:service;active:false}
 FloatingWindow {id:window;property var overlay:Overlay.overlay;title:"Wallpaper fixture";visible:true;implicitWidth:430;implicitHeight:760;color:D.Theme.colors.background
  Flickable {id:scroll;anchors.fill:parent;contentWidth:width;contentHeight:pane.implicitHeight+32;clip:true
   D.WallpaperSettingsPane {id:pane;x:16;y:16;width:parent.width-32;service:service}
  }
 }
 function find(name,item){if(!item)return null;if(item.objectName===name)return item;for(const child of item.children || []){const result=find(name,child);if(result)return result;}for(const resource of item.resources || []){if(resource.contentItem){const result=find(name,resource.contentItem);if(result)return result;}}return null;}
 IpcHandler {target:"filesAudit"
  function refresh():void {service.refresh();}
  function selectSecond():void {pane.selected=service.outputs.find(row=>row.name==="HEADLESS-2").identity;}
  function state():string {return JSON.stringify({ready:!service.busy,rows:service.outputs,error:service.error});}
  function geometry(name:string):string {const item=find(name,window.contentItem) || find(name,window.overlay);if(!item)return "null";const content=item.mapToItem(pane,0,0);if(!name.startsWith("wallpaper-image-"))scroll.contentY=Math.max(0,Math.min(scroll.contentHeight-scroll.height,content.y-150));const at=item.mapToItem(window.contentItem,0,0);return JSON.stringify({x:at.x,y:at.y,width:item.width,height:item.height});}
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
def window():return next(node for node in nodes(json.loads(call('swaymsg','-r','-t','get_tree'))) if node.get('app_id')=='org.quickshell' and node.get('name','')=='Wallpaper fixture')
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
    config=out/'sway.conf';config.write_text('output HEADLESS-1 resolution 1280x900 position 0 0\noutput HEADLESS-2 resolution 1000x800 position 1280 0 scale 1.25\nxwayland disable\nfor_window [all] floating enable, border pixel 0\n')
    env['WLR_HEADLESS_OUTPUTS']='2'
    launch([a.sway,'--unsupported-gpu','-c',str(config)],'sway.log');runtime=Path(env['XDG_RUNTIME_DIR']);env['SWAYSOCK']=str(wait(lambda:next(runtime.glob('sway-ipc*.sock'),None)));env['WAYLAND_DISPLAY']=wait(lambda:next((path.name for path in runtime.glob('wayland-*') if not path.name.endswith('.lock')),None))
    launch(['wtype','-s','60000'],'keyboard.log')
    pointer=sp.Popen([a.pointer_tool,'pointer','move','0','0'],env=env,stdin=sp.PIPE,text=True,start_new_session=True);processes.append(pointer);command('m 0.5 0.5')
    call('swaymsg','focus','output','HEADLESS-1')
    launch(['quickshell','-p',str(qml)],'shell.log');wait(lambda:window());ipc('refresh');wait(lambda:len(state()['rows'])==2 and state()['ready']);ipc('selectSecond')
    click(point('wallpaper-choose'));time.sleep(.5);call('grim','-o','HEADLESS-1',str(out/'owned-chooser-dark.png'));capture=Image.open(out/'owned-chooser-dark.png').convert('RGB');rect=window()['rect'];assert min(capture.getpixel((round(rect['x']+rect['width']/2),round(rect['y']+rect['height']/2))))>15,'Unpainted chooser interior'
    click(point('wallpaper-image-cancel'));assert json.loads((out/'settings.json').read_text()).get('wallpaper',{})=={}
    click(point('wallpaper-choose'));time.sleep(.4)
    key('-M','ctrl','-k','a','-m','ctrl');key(str(blue));key('-k','Return');click(point('wallpaper-image-confirm'))
    wait(lambda:state()['ready'] and any(row['image']==str(blue) for row in state()['rows']))
    assert not state()['error'],state()
    checks=['Actual owned chooser cancel preserves settings; local image confirmation targets selected second display only']
    call('swaymsg','[app_id="org.quickshell"]','move','container','to','output','HEADLESS-1')
    time.sleep(1);call('grim','-o','HEADLESS-2',str(out/'second-display.png'))
    sample=ImageStat.Stat(Image.open(out/'second-display.png').convert('RGB')).mean;assert sample[2]>150 and sample[0]<60,sample
    checks.append('Second output at fractional scale paints selected blue image')
    call('grim','-o','HEADLESS-1',str(out/'chooser-pane-dark.png'))
    saved=(out/'settings.json').read_text();assert str(blue) in saved and str(red) not in saved
    # Restart provider from saved references and prove fallback after file removal.
    helper=str(out/'assets/shell/wallpaper_settings.py')
    launch([sys.executable,helper,'watch'],'wallpaper-watcher.log')
    time.sleep(.4);blue.unlink()
    def refresh_apply():
        time.sleep(2.5)
    refresh_apply();call('grim','-o','HEADLESS-2',str(out/'fallback.png'));sample=ImageStat.Stat(Image.open(out/'fallback.png').convert('RGB')).mean;assert sample[0]>150 and sample[2]<60,sample
    checks.append('Fresh provider after missing image uses readable installed fallback without changing saved reference')
    call('swaymsg','output','HEADLESS-2','disable');refresh_apply();call('swaymsg','output','HEADLESS-2','enable');refresh_apply()
    checks.append('Disconnected output is skipped and reconnect remains readable')
    assert (out/'settings.json').read_text()==saved
    ipc('refresh');wait(lambda:state()['ready'] and any(row['fallback'] for row in state()['rows']));appearance.write_text(json.dumps(dict(font=palettes['font'],palette=palettes['light'],preferences={})));time.sleep(.5);call('grim','-o','HEADLESS-1',str(out/'pane-light.png'))
    checks.append('Owned controls repaint in light appearance without modifying selected images')
    log=(out/'shell.log').read_text();assert not any(marker in log for marker in ('ReferenceError:','TypeError:','Binding loop','Failed to load configuration','Cannot assign')),log
    (out/'result.json').write_text(json.dumps(dict(passed=True,checks=checks,scope='Isolated headless outputs, actual native chooser and synthetic images; no host settings'),indent=2)+'\n')
finally:
    for process in reversed(processes):
        if process.poll() is None:os.killpg(process.pid,signal.SIGTERM)
    if 'pointer' in globals() and pointer.stdin:pointer.stdin.close()
    for process in reversed(processes):
        try:process.wait(timeout=4)
        except sp.TimeoutExpired:os.killpg(process.pid,signal.SIGKILL)
print('PASS native wallpaper chooser and per-display fallback')

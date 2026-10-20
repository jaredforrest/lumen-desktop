"""Genuine nested pointer/keyboard ownership for Files tabs and sidebar layout."""
from fixture_resources import close_reaped_pipes
import argparse,json,os,re,shutil,signal,subprocess as sp,sys,tempfile,time
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--output',type=Path,required=True);p.add_argument('--launcher',type=Path,required=True);p.add_argument('--sway',required=True);p.add_argument('--pointer-tool',required=True);p.add_argument('--inside',action='store_true');a=p.parse_args()
if not a.inside:
    with tempfile.TemporaryDirectory(prefix='lumen-status-input-',ignore_cleanup_errors=True) as private:
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
palettes=json.loads((repo/'res/lumen-desktop/appearance/palette.json').read_text());appearance=out/'appearance.json';appearance.write_text(json.dumps(dict(font=palettes['font'],palette=palettes['dark'],preferences={})));env['LUMEN_APPEARANCE_STATE']=str(appearance)
shutil.copytree(repo/'res/lumen-desktop',out/'assets')
(out/'order.json').write_text('[]')
qml=out/'shell.qml';qml.write_text('''import QtQuick
import Quickshell
import Quickshell.Io
import "assets/shell" as D
ShellRoot {
 id:fixture
 property int activations:0
 property var saved:["battery","sound","focus","search","controls"]
 FloatingWindow {id:window;title:"Status fixture";visible:true;implicitWidth:500;implicitHeight:160;color:D.Theme.colors.background
  D.StatusStrip {id:strip;x:20;y:40;order:fixture.saved;available:({battery:true,sound:true,focus:true,search:true,controls:true});budget:222;onReordered:order=>{fixture.saved=order;store.setText(JSON.stringify(order));};onActivated:fixture.activations++}
 }
 FileView {id:store;path:"SAVED"}
 function find(name,item){if(item.objectName===name)return item;for(const child of item.children || []){const result=find(name,child);if(result)return result;}return null;}
 IpcHandler {target:"filesAudit"
  function state():string {return JSON.stringify({ready:true,order:fixture.saved,visible:strip.layout.visible,overflow:strip.layout.overflow,activations:fixture.activations,width:window.width});}
  function narrow():void {strip.budget=76;}
  function unavailable():void {strip.available={sound:true,search:true,controls:true};}
  function restore():void {strip.budget=222;strip.available={battery:true,sound:true,focus:true,search:true,controls:true};}
  function geometry(name:string):string {const item=fixture.find(name,window.contentItem);if(!item)return "null";const at=item.mapToItem(window.contentItem,0,0);return JSON.stringify({x:at.x,y:at.y,width:item.width,height:item.height});}
 }
}'''.replace('SAVED',str(out/'order.json')))
processes=[]
def launch(command,name):
    with (out/name).open('w') as _fixture_log0:
        process=sp.Popen(command,env=env,stdout=_fixture_log0,stderr=sp.STDOUT,start_new_session=True);processes.append(process);return process

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
def window():return next(node for node in nodes(json.loads(call('swaymsg','-r','-t','get_tree'))) if node.get('app_id')=='org.quickshell' and node.get('name','')=='Status fixture')
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
    launch(['quickshell','-p',str(qml)],'shell.log');wait(lambda:state()['ready'])
    launch(['wtype','-s','60000'],'keyboard.log')
    pointer=sp.Popen([a.pointer_tool,'pointer','move','0','0'],env=env,stdin=sp.PIPE,text=True,start_new_session=True);processes.append(pointer);time.sleep(.2)
    checks=[]
    click(point('status-item-sound'));wait(lambda:state()['activations']==1)
    # Hold a real virtual modifier throughout the pointer gesture.
    first=point('status-item-battery');last=point('status-item-search')
    move(*last);key('-M','logo');command('p')
    for index in range(8):command('m '+str((first[0]-last[0])/8)+' 0')
    command('r');key('-m','logo')
    wait(lambda:state()['order']==['search','battery','sound','focus','controls']);assert state()['activations']==1,state()
    checks.append('Modifier pointer drag reorders without activating a menu')
    key('-M','ctrl','-M','shift','-k','Right','-m','shift','-m','ctrl')
    wait(lambda:state()['order']==['battery','search','sound','focus','controls'])
    checks.append('Real keyboard reorder preserves focused item identity')
    before=state()['order'];move(*point('status-item-search'));key('-M','logo');command('p');command('m 50 0');key('-k','Escape');command('m 10 0');command('r');key('-m','logo');assert state()['order']==before,state()
    checks.append('Escape cancels modifier drag even after further held motion')
    before=state()['order'];move(*point('status-item-search'));key('-M','logo');command('p');command('m 0 60');command('r');key('-m','logo');assert state()['order']==before and state()['activations']==1,state()
    checks.append('Release outside strip cancels without activation or reorder')
    saved=state()['order'];assert json.loads((out/'order.json').read_text())==saved
    checks.append('Presentation order write survives JSON persistence round trip')
    ipc('unavailable');assert state()['order']==saved and state()['visible']==['search','sound','controls'],state()
    ipc('narrow');assert state()['visible']==['sound','controls'] and state()['overflow']==['search'],state()
    ipc('restore');assert state()['order']==saved and len(state()['visible'])==5
    checks.append('Unavailable items retain saved slots and narrow overflow restores complete order')
    call('grim',str(out/'status-strip.png'))
    from PIL import Image,ImageStat
    image=Image.open(out/'status-strip.png').convert('RGB');r=window()['rect'];crop=image.crop((r['x']+20,r['y']+40,r['x']+210,r['y']+64));assert max(ImageStat.Stat(crop).mean)>10 and max(ImageStat.Stat(crop).stddev)>2,'Blank status interior'
    call('swaymsg','output','HEADLESS-1','scale','1.5');time.sleep(.3)
    assert state()['order']==saved and len(state()['visible'])==5,state()
    click(point('status-item-controls'));wait(lambda:state()['activations']==2)
    checks.append('Fractional output scale preserves saved order and usable click target')
    log=(out/'shell.log').read_text();assert not any(marker in log for marker in ('ReferenceError:','TypeError:','Binding loop','Failed to load configuration','Cannot assign')),log
    (out/'result.json').write_text(json.dumps(dict(passed=True,checks=checks,final=state(),scope='Genuine virtual modifier/pointer/keyboard in disposable compositor; no live user input'),indent=2)+'\n')
finally:
    for process in reversed(processes):
        if process.poll() is None:os.killpg(process.pid,signal.SIGTERM)
    for process in reversed(processes):
        try:process.wait(timeout=4)
        except sp.TimeoutExpired:os.killpg(process.pid,signal.SIGKILL)
    for _fixture_process in processes:
        if _fixture_process.poll() is None:_fixture_process.wait(timeout=5)
    close_reaped_pipes(processes)
print('PASS native status strip modifier pointer and keyboard reorder')

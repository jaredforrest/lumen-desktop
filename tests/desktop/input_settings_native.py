"""Per-device keyboard preferences on genuine disposable virtual input devices."""
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
qml=out/'shell.qml';qml.write_text('''import QtQuick
import QtQuick.Window
import Quickshell
import Quickshell.Io
import "assets/shell" as D
ShellRoot {
 D.InputSettingsState {id:service;active:false}
 FloatingWindow {id:window;title:"Input fixture";visible:true;implicitWidth:420;implicitHeight:700;color:D.Theme.colors.background
  Flickable {id:scroll;anchors.fill:parent;contentWidth:width;contentHeight:pane.implicitHeight+32;clip:true
   D.InputSettingsPane {id:pane;x:16;y:16;width:parent.width-32;service:service}
  }
 }
 function find(name,item){if(item.objectName===name)return item;for(const child of item.children || []){const result=find(name,child);if(result)return result;}return null;}
 IpcHandler {target:"filesAudit"
  function refresh():void {service.refresh();}
  function selectKeyboard():void {pane.selected=service.devices.find(row=>row.identity.type==="keyboard").key;}
  function state():string {return JSON.stringify({ready:service.available && !service.busy,rows:service.devices,selected:pane.device,error:service.error,message:service.message});}
  function geometry(name:string):string {const item=find(name,window.contentItem);if(!item)return "null";const content=item.mapToItem(pane,0,0);scroll.contentY=Math.max(0,Math.min(scroll.contentHeight-scroll.height,content.y-150));const at=item.mapToItem(window.contentItem,0,0);return JSON.stringify({x:at.x,y:at.y,width:item.width,height:item.height});}
 }
}''')
processes=[]
def launch(command,name):
    process=sp.Popen(command,env=env,stdout=(out/name).open('w'),stderr=sp.STDOUT,start_new_session=True);processes.append(process);return process

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
def window():return next(node for node in nodes(json.loads(call('swaymsg','-r','-t','get_tree'))) if node.get('app_id')=='org.quickshell' and node.get('name','')=='Input fixture')
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
    launch(['wtype','-s','60000'],'keyboard.log')
    pointer=sp.Popen([a.pointer_tool,'pointer','move','0','0'],env=env,stdin=sp.PIPE,text=True,start_new_session=True);processes.append(pointer);command('m 0.5 0.5')
    launch(['quickshell','-p',str(qml)],'shell.log');wait(lambda:window())
    ipc('refresh');wait(lambda:state()['ready'] and any(row['identity']['type']=='keyboard' for row in state()['rows']));ipc('selectKeyboard');time.sleep(.1)
    checks=[];before=json.loads(call('swaymsg','-r','-t','get_inputs'));keyboard=next(device for device in before if device['type']=='keyboard');previous=keyboard['repeat_rate']
    click(point('input-slider-repeat_rate',.8));wait(lambda:state()['ready'] and state()['selected']['saved'])
    after=json.loads(call('swaymsg','-r','-t','get_inputs'));changed=next(device for device in after if device['identifier']==keyboard['identifier']);assert changed['repeat_rate']!=previous and not state()['error'],state()
    (out/'devices-before.json').write_text(json.dumps(before,indent=2));(out/'devices-after.json').write_text(json.dumps(after,indent=2))
    assert [device for device in before if device['type']!='keyboard']==[device for device in after if device['type']!='keyboard']
    record=json.loads((out/'settings.json').read_text())['inputs'][state()['selected']['key']];assert record['values']['repeat_rate']==changed['repeat_rate'] and record['baseline']['repeat_rate']==previous,record
    checks.append('Actual slider changes only the uniquely identified disposable keyboard and persists its previous value')
    call('grim',str(out/'input-keyboard-dark.png'))
    click(point('input-forget-profile'));click(point('input-forget-cancel'));assert state()['selected']['saved']
    checks.append('Actual Forget confirmation can be cancelled without changing the profile or current device')
    click(point('input-restore-repeat_rate'));wait(lambda:state()['ready'] and not state()['selected']['saved']);restored=json.loads(call('swaymsg','-r','-t','get_inputs'));assert next(device for device in restored if device['identifier']==keyboard['identifier'])['repeat_rate']==previous
    checks.append('Actual Restore button reverses the device setting and removes its saved override')
    from PIL import Image,ImageStat
    image=Image.open(out/'input-keyboard-dark.png').convert('RGB');r=window()['rect'];crop=image.crop((r['x']+10,r['y']+20,r['x']+r['width']-10,r['y']+r['height']-10));assert max(ImageStat.Stat(crop).mean)>10 and max(ImageStat.Stat(crop).stddev)>2,'Blank input pane'
    log=(out/'shell.log').read_text();assert not any(marker in log for marker in ('ReferenceError:','TypeError:','Binding loop','Failed to load configuration','Cannot assign')),log
    (out/'result.json').write_text(json.dumps(dict(passed=True,checks=checks,final=state(),scope='Genuine isolated compositor and virtual keyboard/pointer; disposable settings adapter; no host device changes'),indent=2)+'\n')
finally:
    for process in reversed(processes):
        if process.poll() is None:os.killpg(process.pid,signal.SIGTERM)
    for process in reversed(processes):
        try:process.wait(timeout=4)
        except sp.TimeoutExpired:os.killpg(process.pid,signal.SIGKILL)
print('PASS native per-device input preference and reversal on disposable virtual keyboard')

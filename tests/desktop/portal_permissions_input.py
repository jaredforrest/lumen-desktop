"""Actual permission confirmation clicks and painted light/dark owned pane."""
from fixture_resources import close_reaped_pipes
import argparse,json,os,re,shutil,signal,subprocess as sp,sys,tempfile,time
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--output',type=Path,required=True);p.add_argument('--launcher',type=Path,required=True);p.add_argument('--sway',required=True);p.add_argument('--pointer-tool',required=True);p.add_argument('--inside',action='store_true');a=p.parse_args()
if not a.inside:
    with tempfile.TemporaryDirectory(prefix='lumen-portal-input-',ignore_cleanup_errors=True) as private:
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
palettes=json.loads((repo/'res/lumen-desktop/appearance/palette.json').read_text());appearance=out/'appearance.json';appearance.write_text(json.dumps(dict(font=palettes['font'],palette=palettes['dark'],preferences={})));env['LUMEN_APPEARANCE_STATE']=str(appearance);env['GLOBAL_MENU_PYTHON']=sys.executable
shutil.copytree(repo/'res/lumen-desktop',out/'assets')
qml=out/'shell.qml';qml.write_text('''import QtQuick
import Quickshell
import Quickshell.Io
import "assets/shell" as D
ShellRoot {
 id:fixture
 D.PortalPermissionState {id:service;active:true}
 FloatingWindow {id:window;title:"Permission fixture";visible:true;implicitWidth:420;implicitHeight:820;color:D.Theme.colors.background
  Flickable {id:scroll;anchors.fill:parent;contentWidth:width;contentHeight:pane.implicitHeight+32;clip:true
   D.PortalPermissionsPane {id:pane;x:16;y:16;width:parent.width-32;service:service}
  }
 }
 function find(name,item){if(item.objectName===name)return item;for(const child of item.children || []){const result=find(name,child);if(result)return result;}return null;}
 IpcHandler {target:"filesAudit"
  function state():string {return JSON.stringify({ready:service.facts.available && !service.busy,rows:service.facts.rows,pending:pane.pending,error:service.error,message:service.message,width:window.width,dark:D.Theme.dark});}
  function geometry(name:string):string {const item=fixture.find(name,window.contentItem);if(!item)return "null";const content=item.mapToItem(pane,0,0);scroll.contentY=Math.max(0,Math.min(scroll.contentHeight-scroll.height,content.y-100));const at=item.mapToItem(window.contentItem,0,0);return JSON.stringify({x:at.x,y:at.y,width:item.width,height:item.height});}
 }
}''')
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
def window():return next(node for node in nodes(json.loads(call('swaymsg','-r','-t','get_tree'))) if node.get('app_id')=='org.quickshell' and node.get('name','')=='Permission fixture')
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
    ready=out/'provider-ready';launch([sys.executable,str(repo/'tests/desktop/fixtures/portal_permission_mock.py'),'--ready',str(ready)],'provider.log');wait(lambda:ready.exists())
    launch(['quickshell','-p',str(qml)],'shell.log');wait(lambda:state()['ready'])
    launch(['wtype','-s','60000'],'keyboard.log')
    pointer=sp.Popen([a.pointer_tool,'pointer','move','0','0'],env=env,stdin=sp.PIPE,text=True,start_new_session=True);processes.append(pointer);time.sleep(.2)
    checks=[]
    def camera():return next(row for row in state()['rows'] if row['capability']=='Camera' and row['appId']=='org.example.First')
    row=camera();click(point('portal-permission-deny-'+row['id']));wait(lambda:state()['pending'] is not None)
    assert state()['pending']['id']==row['id'] and camera()['decision']=='Allowed',state()
    call('grim',str(out/'confirmation-dark.png'))
    click(point('portal-permission-confirm'));wait(lambda:state()['ready'] and camera()['decision']=='Denied')
    assert not state()['error'] and 'Existing capture may continue' in state()['message'],state()
    assert next(item for item in state()['rows'] if item['capability']=='Camera' and item['appId']=='org.example.Second')['decision']=='Denied'
    checks.append('Actual pointer action and exact-target confirmation change only the intended provider decision')
    from PIL import Image,ImageStat
    for mode in ('dark','light'):
        pending=appearance.with_suffix('.new');pending.write_text(json.dumps(dict(font=palettes['font'],palette=palettes[mode],preferences={})));pending.replace(appearance)
        wait(lambda:state()['dark']==(mode=='dark'));time.sleep(.25)
        call('grim',str(out/('permissions-'+mode+'.png')))
        image=Image.open(out/('permissions-'+mode+'.png')).convert('RGB');r=window()['rect'];crop=image.crop((r['x']+10,r['y']+20,r['x']+r['width']-10,r['y']+r['height']-10));assert max(ImageStat.Stat(crop).mean)>10 and max(ImageStat.Stat(crop).stddev)>2,'Blank permission pane'
    checks.append('Actual light/dark pane paints readable interior after live palette changes')
    log=(out/'shell.log').read_text();assert not any(marker in log for marker in ('ReferenceError:','TypeError:','Binding loop','Failed to load configuration','Cannot assign')),log
    assert (out/'provider.log').read_text()==''
    (out/'result.json').write_text(json.dumps(dict(passed=True,checks=checks,final=state(),scope='Private D-Bus provider and genuine nested pointer; no account grants or live input'),indent=2)+'\n')
finally:
    for process in reversed(processes):
        if process.poll() is None:os.killpg(process.pid,signal.SIGTERM)
    for process in reversed(processes):
        try:process.wait(timeout=4)
        except sp.TimeoutExpired:os.killpg(process.pid,signal.SIGKILL)
    for _fixture_process in processes:
        if _fixture_process.poll() is None:_fixture_process.wait(timeout=5)
    close_reaped_pipes(processes)
print('PASS native permission confirmation and painted light/dark pane')

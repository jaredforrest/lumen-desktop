"""Exercise actual modifier release and cancellation in a private headless session."""
import argparse
import json
import os
from pathlib import Path
import shutil
import signal
import subprocess
import sys
import tempfile
import time

parser=argparse.ArgumentParser()
parser.add_argument('--sway',required=True)
parser.add_argument('--wtype',required=True)
parser.add_argument('--output',type=Path,required=True)
parser.add_argument('--inside',action='store_true')
args=parser.parse_args()
if not args.inside:
    raise SystemExit(subprocess.call(['dbus-run-session','--',sys.executable,__file__,*sys.argv[1:],'--inside']))
root=args.output.resolve();root.mkdir(parents=True,exist_ok=False)
runtime=Path(tempfile.mkdtemp(prefix='lumen-switcher-'))
env=dict(os.environ,XDG_RUNTIME_DIR=str(runtime),WLR_BACKENDS='headless',WLR_RENDERER='pixman',WLR_LIBINPUT_NO_DEVICES='1',QT_QPA_PLATFORM='wayland',QT_QUICK_BACKEND='software',GDK_BACKEND='wayland',QT_QPA_PLATFORMTHEME='')
env.update(QT_ACCESSIBILITY='0',QT_LINUX_ACCESSIBILITY_ALWAYS_ON='0',NO_AT_BRIDGE='1',GTK_A11Y='none')
env.pop('AT_SPI_BUS_ADDRESS',None)
for key in ('WAYLAND_DISPLAY','SWAYSOCK','DISPLAY'):env.pop(key,None)
for key in ('CONFIG','CACHE','STATE','DATA'):env['XDG_'+key+'_HOME']=str(root/key.lower())
source=Path(__file__).resolve().parents[2]/'res/lumen-desktop/shell'
fixture=root/'fixture';fixture.mkdir()
for name in ('AppSwitcher.qml','AppSwitcherModel.js'):shutil.copy(source/name,fixture/name)
(fixture/'qmldir').write_text('singleton Theme 1.0 Theme.qml\nAppSwitcher 1.0 AppSwitcher.qml\nAppIcon 1.0 AppIcon.qml\n')
(fixture/'Theme.qml').write_text('pragma Singleton\nimport QtQuick\nQtObject {property string font:"Sans";property string outline:"#999999";property var colors:({background:"#eeeeee",outline:"#999999",surface:"#ffffff",accent:"#0088ff",text:"#000000"})}')
(fixture/'AppIcon.qml').write_text('import QtQuick\nRectangle {property var iconSource;color:"#ffffff"}')
(fixture/'shell.qml').write_text('''import QtQuick
import Quickshell
import Quickshell.Io
import "."
ShellRoot {id:mock
 property var windows:[{appId:"a",title:"A"},{appId:"b",title:"B"},{appId:"a",title:"A2"}]
 property var activationHistory:windows
 property string popupOwner:""
 property int focusEpoch:0
 property int activated:0
 property string activatedApp:""
 signal dismissPopups(string owner)
 function entryFor(w){return null;}
 function nameForAppId(a,b){return a;}
 function preferredOutput(){return "";}
 function claimPopup(owner){++focusEpoch;popupOwner=owner;dismissPopups(owner);}
 function focusWindow(w){activated++;activatedApp=w.appId;Quickshell.execDetached(["swaymsg",'[app_id="'+w.appId+'"] focus']);}
 AppSwitcher {id:picker;shell:mock}
 IpcHandler {target:"fixture"
 function step(action:string):string {picker.command(action);return picker.status();}
 function raceEpoch():void {picker.command("next");picker.command("commit");++mock.focusEpoch;}
 function raceRemovedApp():void {picker.command("next");picker.command("commit");mock.windows=mock.windows.filter(w=>w.appId!=="b");}
 function status():string {return JSON.stringify({picker:JSON.parse(picker.status()),activated:mock.activated,app:mock.activatedApp});}
 }
}''')
qs=shutil.which('quickshell')
config=root/'sway.conf'
# The binding has no build-specific QML path. Model a rebuilt service whose
# PID changes while the compositor keeps its existing binding wrapper.
service_pid=root/'service.pid'
service_control=root/'systemctl-fixture'
service_control.write_text('#!'+shutil.which('bash')+'\ncat '+str(service_pid)+'\n')
service_control.chmod(0o700)
ipc_source=Path(__file__).resolve().parents[2]/'res/lumen-desktop/runtime/shell_ipc.py'
binding=' '.join([sys.executable,str(ipc_source),'--systemctl',str(service_control),'--quickshell',qs,'--','call','fixture','step'])
config.write_text('output HEADLESS-1 resolution 1000x700\nseat seat0 fallback true\nbindsym Mod4+Tab exec '+binding+' next\n'+'bindsym --release Super_L exec '+binding+' commit\n'+'bindsym --release Super_R exec '+binding+' commit\n')
processes=[]
def launch(command,name):
    with (root/name).open('w') as _fixture_log0:
        process=subprocess.Popen(command,env=env,stdout=_fixture_log0,stderr=subprocess.STDOUT,start_new_session=True)
    processes.append(process);return process

def wait(callback):
    for _ in range(100):
        try:
            value=callback()
            if value:return value
        except (OSError,ValueError,subprocess.SubprocessError):pass
        time.sleep(.05)
    raise AssertionError('Private session did not reach expected state')

def ipc(*parts):return subprocess.check_output([qs,'-p',str(fixture),'ipc','call','fixture',*parts],env=env,text=True,timeout=5)
def status():return json.loads(ipc('status'))
try:
    launch([args.sway,'--unsupported-gpu','-c',str(config)],'sway.log')
    wait(lambda:list(runtime.glob('sway-ipc.*.sock')))
    env['SWAYSOCK']=str(next(runtime.glob('sway-ipc.*.sock')))
    env['WAYLAND_DISPLAY']=wait(lambda:next((p.name for p in runtime.glob('wayland-*') if not p.name.endswith('.lock')),None))
    launch(['foot','--app-id=a','--title=A','sh','-c','sleep 600'],'app-a.log')
    launch(['foot','--app-id=b','--title=B','sh','-c','sleep 600'],'app-b.log')
    def leaves(node):
        return ([node] if node.get('app_id') else [])+sum([leaves(n) for n in node.get('nodes',[])+node.get('floating_nodes',[])],[])
    def app_windows():return leaves(json.loads(subprocess.check_output(['swaymsg','-t','get_tree','-r'],env=env,text=True,timeout=5)))
    wait(lambda:len(app_windows())==2)
    subprocess.run(['swaymsg','[app_id="a"] focus'],env=env,check=True,stdout=subprocess.DEVNULL)
    def focused_app():return next((w['app_id'] for w in app_windows() if w.get('focused')),None)
    assert focused_app()=='a'
    shell_process=launch([qs,'-p',str(fixture)],'shell.log')
    service_pid.write_text(str(shell_process.pid))
    wait(lambda:status()['picker']['phase']=='closed')
    # One virtual keyboard holds Super throughout invocation/repeat/release.
    subprocess.run([args.wtype,'-M','logo','-P','Super_L','-k','Tab','-s','350','-p','Super_L','-m','logo'],env=env,check=True,timeout=5)
    first=wait(lambda:(s if (s:=status())['activated']==1 else None))
    assert first['app']=='b' and first['picker']['phase']=='committed',first
    wait(lambda:focused_app()=='b')
    subprocess.run([args.wtype,'-M','logo','-P','Super_L','-k','Tab','-s','250','-k','Tab','-s','250','-p','Super_L','-m','logo'],env=env,check=True,timeout=5)
    second=wait(lambda:(s if (s:=status())['activated']==2 else None))
    assert second['app']=='a',second
    wait(lambda:focused_app()=='a')
    subprocess.run([args.wtype,'-M','logo','-P','Super_L','-k','Tab','-s','250','-k','Escape','-s','100','-p','Super_L','-m','logo'],env=env,check=True,timeout=5)
    cancelled=wait(lambda:(s if (s:=status())['picker']['phase']=='cancelled' else None))
    assert cancelled['activated']==2,cancelled
    wait(lambda:focused_app()=='a')
    ipc('raceEpoch');epoch_cancelled=wait(lambda:(s if (s:=status())['picker']['phase']=='cancelled' else None))
    assert epoch_cancelled['activated']==2,epoch_cancelled
    ipc('raceRemovedApp');removed_cancelled=wait(lambda:(s if (s:=status())['picker']['phase']=='cancelled' else None))
    assert removed_cancelled['activated']==2,removed_cancelled
    # Rebuild/restart the service from another immutable-path analogue while
    # deliberately leaving Sway's binding unchanged. Actions must follow it.
    os.killpg(shell_process.pid,signal.SIGTERM);shell_process.wait(timeout=3)
    rebuilt=root/'rebuilt-assets';shutil.copytree(fixture,rebuilt);fixture=rebuilt
    shell_process=launch([qs,'-p',str(fixture)],'rebuilt-shell.log')
    service_pid.write_text(str(shell_process.pid))
    wait(lambda:status()['picker']['phase']=='closed')
    subprocess.run([args.wtype,'-M','logo','-P','Super_L','-k','Tab','-s','350','-p','Super_L','-m','logo'],env=env,check=True,timeout=5)
    after_rebuild=wait(lambda:(s if (s:=status())['activated']==1 else None))
    assert after_rebuild['app']=='b' and after_rebuild['picker']['phase']=='committed',after_rebuild
    (root/'result.json').write_text(json.dumps(dict(first=first,second=second,cancelled=cancelled,epoch_cancelled=epoch_cancelled,removed_cancelled=removed_cancelled,after_rebuild=after_rebuild),indent=2))
    print('PASS real Super release, app grouping, repeat and cancellation without preview activation')
finally:
    for process in reversed(processes):
        try:os.killpg(process.pid,signal.SIGTERM)
        except ProcessLookupError:pass
    for process in processes:
        try:process.wait(timeout=3)
        except subprocess.TimeoutExpired:os.killpg(process.pid,signal.SIGKILL)
    shutil.rmtree(runtime,ignore_errors=True)

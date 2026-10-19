"""Render and cancel a simulated confirmation in a private Wayland session."""
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

parser=argparse.ArgumentParser();parser.add_argument('--sway',required=True);parser.add_argument('--wtype',required=True);parser.add_argument('--output',type=Path,required=True);parser.add_argument('--inside',action='store_true');args=parser.parse_args()
if not args.inside:raise SystemExit(subprocess.call(['dbus-run-session','--',sys.executable,__file__,*sys.argv[1:],'--inside']))
root=args.output.resolve();root.mkdir(parents=True,exist_ok=False);runtime=Path(tempfile.mkdtemp(prefix='lumen-quit-'))
env=dict(os.environ,HOME=str(root/'home'),XDG_RUNTIME_DIR=str(runtime),WLR_BACKENDS='headless',WLR_RENDERER='pixman',WLR_LIBINPUT_NO_DEVICES='1',QT_QPA_PLATFORM='wayland',QT_QUICK_BACKEND='software',QT_QPA_PLATFORMTHEME='',QT_ACCESSIBILITY='0',QT_LINUX_ACCESSIBILITY_ALWAYS_ON='0',NO_AT_BRIDGE='1',GTK_A11Y='none')
for key in ('WAYLAND_DISPLAY','SWAYSOCK','DISPLAY','AT_SPI_BUS_ADDRESS'):env.pop(key,None)
for key in ('CONFIG','CACHE','STATE','DATA'):env['XDG_'+key+'_HOME']=str(root/key.lower())
source=Path(__file__).resolve().parents[2]/'res/lumen-desktop/shell';fixture=root/'fixture';fixture.mkdir()
helper=root/'lumen-application-control';log=root/'requests.jsonl'
helper.write_text('#!'+sys.executable+'\nimport json,sys\nfrom pathlib import Path\nrequest=sys.argv[2];action=sys.argv[3]\nwith Path('+repr(str(log))+').open("a") as output:output.write(json.dumps(dict(request=request,action=action))+"\\n")\nprint(json.dumps(dict(requestId=request,token="0"*32,windowCount=2,cancelled=True)))\n');helper.chmod(0o700)
env['PATH']=str(root)+os.pathsep+env['PATH']
(fixture/'shell.qml').write_text('''import QtQuick
import Quickshell
import Quickshell.Io
import "'''+source.as_uri()+'''" as Desktop
ShellRoot {id:mock
 property string popupOwner:""
 signal dismissPopups(string owner)
 function preferredOutput(){return "";}
 function claimPopup(owner){popupOwner=owner;dismissPopups(owner);}
 Desktop.ForceQuitDialog {id:dialog;shell:mock}
 IpcHandler {target:"fixture"
 function openDialog(dark:bool):void {Desktop.Theme.colors=dark?({background:"#242426",field:"#343438",text:"#eeeeee",muted:"#aaaaaa",hover:"#454549",accent:"#0a84ff"}):({background:"#f2f2f4",field:"#ffffff",text:"#202023",muted:"#6b6b70",hover:"#dedee3",accent:"#007aff"});dialog.show("Disposable Fixture",[{appId:"fixture",title:"Fixture"}]);}
 function status():string {return JSON.stringify({phase:dialog.phase,visible:dialog.visible});}
 }
}''')
config=root/'sway.conf';config.write_text('output HEADLESS-1 resolution 900x650\nseat seat0 fallback true\n')
processes=[]
def launch(command,name):
    with (root/name).open('w') as stream:
        process=subprocess.Popen(command,env=env,stdout=stream,stderr=subprocess.STDOUT,start_new_session=True)
    processes.append(process);return process

def wait(callback):
    for _ in range(100):
        try:
            value=callback()
            if value:return value
        except (OSError,ValueError,subprocess.SubprocessError):pass
        time.sleep(.05)
    raise AssertionError('Private confirmation did not reach expected state')
qs=shutil.which('quickshell')
def ipc(*parts):return subprocess.check_output([qs,'-p',str(fixture),'ipc','call','fixture',*parts],env=env,text=True,timeout=5)
def status():return json.loads(ipc('status'))
try:
    launch([args.sway,'--unsupported-gpu','-c',str(config)],'sway.log')
    wait(lambda:list(runtime.glob('sway-ipc.*.sock')));env['SWAYSOCK']=str(next(runtime.glob('sway-ipc.*.sock')))
    env['WAYLAND_DISPLAY']=wait(lambda:next((p.name for p in runtime.glob('wayland-*') if not p.name.endswith('.lock')),None))
    launch([args.wtype,'-s','30000'],'keyboard.log');time.sleep(.2)
    launch([qs,'-p',str(fixture)],'shell.log');wait(lambda:status()['phase']=='closed')
    for theme in ['light','dark']:
        ipc('openDialog','true' if theme=='dark' else 'false');wait(lambda:status()['phase']=='ready')
        time.sleep(.15)
        subprocess.run(['grim',str(root/(theme+'.png'))],env=env,check=True,timeout=5)
        subprocess.run([args.wtype,'-k','Escape'],env=env,check=True,timeout=5)
        wait(lambda:not status()['visible'])
    def cancelled():
        return log.exists() and len([line for line in log.read_text().splitlines() if json.loads(line)['action']=='cancel'])==2
    wait(cancelled)
    calls=[json.loads(line) for line in log.read_text().splitlines()]
    assert all(call['action']!='confirm' for call in calls),calls
    (root/'result.json').write_text(json.dumps({'passed':True,'themes':['light','dark'],'escapeCancelled':2,'terminationRequests':0},indent=2));print('PASS private Force Quit light/dark surfaces and Escape cancellation')
finally:
    for process in reversed(processes):
        try:os.killpg(process.pid,signal.SIGTERM)
        except ProcessLookupError:pass
    for process in processes:
        try:process.wait(timeout=3)
        except subprocess.TimeoutExpired:
            os.killpg(process.pid,signal.SIGKILL);process.wait(timeout=3)
    shutil.rmtree(runtime,ignore_errors=True)

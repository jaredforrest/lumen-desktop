"""Exercise Dock geometry and edge reveal only inside a private compositor."""
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

parser=argparse.ArgumentParser();parser.add_argument('--sway',required=True);parser.add_argument('--pointer',required=True);parser.add_argument('--output',type=Path,required=True);parser.add_argument('--inside',action='store_true');args=parser.parse_args()
if not args.inside:raise SystemExit(subprocess.call(['dbus-run-session','--',sys.executable,__file__,*sys.argv[1:],'--inside']))
root=args.output.resolve();root.mkdir(parents=True,exist_ok=False)
runtime=Path(tempfile.mkdtemp(prefix='lumen-dock-'))
env=dict(os.environ,XDG_RUNTIME_DIR=str(runtime),WLR_BACKENDS='headless',WLR_RENDERER='pixman',WLR_LIBINPUT_NO_DEVICES='1',QT_QPA_PLATFORM='wayland',QT_QUICK_BACKEND='software',QT_QPA_PLATFORMTHEME='',QT_ACCESSIBILITY='0',QT_LINUX_ACCESSIBILITY_ALWAYS_ON='0',NO_AT_BRIDGE='1',GTK_A11Y='none')
for key in ('WAYLAND_DISPLAY','SWAYSOCK','DISPLAY','AT_SPI_BUS_ADDRESS'):env.pop(key,None)
for key in ('CONFIG','CACHE','STATE','DATA'):env['XDG_'+key+'_HOME']=str(root/key.lower())
source=Path(__file__).resolve().parents[2]/'res/lumen-desktop/shell'
fixture=root/'fixture';fixture.mkdir()
(fixture/'shell.qml').write_text('''import QtQuick
import Quickshell
import Quickshell.Io
import "'''+source.as_uri()+'''" as Desktop
ShellRoot {id:mock
 property string popupOwner:""
 property bool dockPreferencesBusy:false
 property var currentDate:new Date()
 property int activated:0
 property var referenceDock:[{key:"a",entry:{id:"a",name:"A",icon:"utilities-terminal"},canvas:68,image:""},{key:"b",entry:{id:"b",name:"B",icon:"utilities-terminal"},canvas:68,image:""},{key:"downloads",name:"Downloads",action:"downloads",canvas:68,image:""}]
 signal requestDockFocus(string name)
 signal dismissPopups(string owner)
 function claimPopup(owner){popupOwner=owner;dismissPopups(owner);}
 function dockWindows(item){return [];}
 function dockLabel(item){return item.name || item.entry.name;}
 function activateDock(item){activated++;}
 function isDockPinned(entry){return true;}
 function canConfigureDockEntry(entry){return true;}
 function windowsFor(entry){return [];}
 function updateDockPin(entry,action){}
 function reorderDockPin(a,b){}
 Desktop.Dock {id:dock;screen:Quickshell.screens[0];shell:mock}
 IpcHandler {target:"fixture"
 function configure(position:string,autoHide:bool,size:int):void {Desktop.Theme.preferences=Object.assign({},Desktop.Theme.preferences,{dockPosition:position,dockAutoHide:autoHide,dockSize:size,reduceMotion:true});}
 function status():string {return JSON.stringify({width:dock.width,height:dock.height,slot:dock.slot,iconSize:dock.iconSize,vertical:dock.vertical,position:dock.position,revealed:dock.revealed,hovered:dock.hovered,autoHide:dock.autoHide,zone:dock.exclusiveZone,activated:mock.activated});}
 }
}''')
config=root/'sway.conf';config.write_text('output HEADLESS-1 resolution 1000x700\nseat seat0 fallback true\n')
processes=[]
def launch(command,name):
    process=subprocess.Popen(command,env=env,stdout=(root/name).open('w'),stderr=subprocess.STDOUT,start_new_session=True);processes.append(process);return process

def wait(callback):
    for _ in range(120):
        try:
            value=callback()
            if value:return value
        except (OSError,ValueError,subprocess.SubprocessError):pass
        time.sleep(.05)
    raise AssertionError('Private Dock fixture did not reach expected state')
qs=shutil.which('quickshell')
def ipc(*parts):return subprocess.check_output([qs,'-p',str(fixture),'ipc','call','fixture',*parts],env=env,text=True,timeout=5)
def status():return json.loads(ipc('status'))
def move(x,y):
    subprocess.run(['swaymsg',f'seat seat0 cursor set {x} {y}'],env=env,check=True,timeout=5,stdout=subprocess.DEVNULL)
    time.sleep(.1)

try:
    launch([args.sway,'--unsupported-gpu','-c',str(config)],'sway.log')
    wait(lambda:list(runtime.glob('sway-ipc.*.sock')));env['SWAYSOCK']=str(next(runtime.glob('sway-ipc.*.sock')))
    env['WAYLAND_DISPLAY']=wait(lambda:next((p.name for p in runtime.glob('wayland-*') if not p.name.endswith('.lock')),None))
    launch([args.pointer,'pointer','move','0','0'],'pointer.log');time.sleep(.2)
    launch([qs,'-p',str(fixture)],'shell.log');wait(lambda:status()['width']>0)
    results=[]
    for position in ['bottom','left','right']:
        for size in [32,48,72]:
            ipc('configure',position,'false',str(size))
            state=wait(lambda:(s if (s:=status())['position']==position and abs(s['iconSize']-size)<1 else None))
            assert (state['height']>state['width'])==state['vertical'],state
            assert state['zone']>=size+26,state
            results.append(state)
        ipc('configure',position,'true','48');move(500,350)
        hidden=wait(lambda:(s if not (s:=status())['revealed'] else None));assert hidden['zone']==0,hidden
        move(*({'bottom':(500,699),'left':(0,350),'right':(999,350)}[position]))
        shown=wait(lambda:(s if (s:=status())['revealed'] else None))
        move(*({'bottom':(500,663),'left':(37,350),'right':(963,350)}[position]))
        shown=wait(lambda:(s if (s:=status())['hovered']>=0 else None))
        assert shown['activated']==0,shown
        if shutil.which('grim'):subprocess.run(['grim',str(root/(position+'.png'))],env=env,check=True,timeout=5)
        move(500,350);wait(lambda:not status()['revealed'])
    (root/'result.json').write_text(json.dumps({'passed':True,'geometry':results},indent=2));print('PASS Dock edge placement, size, input and automatic hiding')
finally:
    for process in reversed(processes):
        try:os.killpg(process.pid,signal.SIGTERM)
        except ProcessLookupError:pass
    for process in processes:
        try:process.wait(timeout=3)
        except subprocess.TimeoutExpired:os.killpg(process.pid,signal.SIGKILL)
    shutil.rmtree(runtime,ignore_errors=True)

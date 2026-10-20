"""Keep real owned windows open while Theme watches atomic palette replacements."""
import argparse
from collections import Counter
import importlib.util
import json
import os
from pathlib import Path
import re
import shutil
import signal
import subprocess as sp
import sys
import tempfile
import time

p=argparse.ArgumentParser();p.add_argument('--launcher',type=Path,required=True);p.add_argument('--sway',required=True);p.add_argument('--output',type=Path,required=True);p.add_argument('--inside',action='store_true');a=p.parse_args()
if not a.inside:
    with tempfile.TemporaryDirectory(prefix='lumen-live-appearance-') as private:
        env=dict(os.environ,HOME=private,XDG_RUNTIME_DIR=private,DBUS_SYSTEM_BUS_ADDRESS='unix:path='+str(Path(private)/'no-system-bus'))
        for key in ('CONFIG','STATE','CACHE','DATA'):env['XDG_'+key+'_HOME']=str(Path(private)/key.lower())
        for key in ('DISPLAY','WAYLAND_DISPLAY','SWAYSOCK','DBUS_SESSION_BUS_ADDRESS','AT_SPI_BUS_ADDRESS'):env.pop(key,None)
        config=Path(private)/'bus.conf'
        config.write_text('<busconfig><type>session</type><listen>unix:tmpdir='+private+'</listen><auth>EXTERNAL</auth><policy context="default"><allow send_destination="*"/><allow receive_sender="*"/><allow own="*"/></policy></busconfig>')
        raise SystemExit(sp.call(['dbus-run-session','--config-file='+str(config),'--',sys.executable,__file__,*sys.argv[1:],'--inside'],env=env))
repo=Path(__file__).resolve().parents[2];out=a.output.resolve();out.mkdir(parents=True,exist_ok=False)
prefix=re.split(r'\n[ \t]*exec quickshell ',a.launcher.read_text(),maxsplit=1)[0]
exported=sp.check_output(['bash','-c',prefix+'\nexec env -0']);env=dict(value.decode().split('=',1) for value in exported.split(b'\0') if b'=' in value)
env.update(WLR_BACKENDS='headless',WLR_RENDERER='pixman',WLR_LIBINPUT_NO_DEVICES='1',QT_QPA_PLATFORM='wayland',QT_QPA_PLATFORMTHEME='generic',QT_QUICK_BACKEND='software',QT_ACCESSIBILITY='0',QT_LINUX_ACCESSIBILITY_ALWAYS_ON='0',NO_AT_BRIDGE='1',GTK_A11Y='none',LUMEN_NATIVE_WINDOW_CONTROLS='0',LUMEN_NATIVE_WINDOW_ACTIONS='0',LUMEN_UNIFIED_OWNED_HEADERS='1')
for key in ('DISPLAY','WAYLAND_DISPLAY','SWAYSOCK','AT_SPI_BUS_ADDRESS'):env.pop(key,None)
spec=importlib.util.spec_from_file_location('live_palette',repo/'res/lumen-desktop/appearance/appearance.py');appearance=importlib.util.module_from_spec(spec);spec.loader.exec_module(appearance)
palettes=json.loads((repo/'res/lumen-desktop/appearance/palette.json').read_text());appearance_path=out/'appearance.json';env['LUMEN_APPEARANCE_STATE']=str(appearance_path)
def theme(mode):
    colors=appearance.resolve_palette(palettes,mode,appearance.DEFAULTS)
    temporary=appearance_path.with_suffix('.tmp');temporary.write_text(json.dumps(dict(font=palettes['font'],palette=colors,preferences=dict(mode=mode,reduceMotion=True))));temporary.replace(appearance_path)
    return colors

theme('light');qml=out/'shell.qml'
shutil.copytree(repo/'res/lumen-desktop',out/'assets')
qml.write_text('''import QtQuick
import Quickshell
import Quickshell.Io
import "assets/shell" as D
ShellRoot {
 QtObject {id:stub;property string popupOwner:"";signal dismissPopups(string owner);function claimPopup(owner){popupOwner=owner;dismissPopups(owner);}}
 D.HelpWindow {id:help;shell:stub}
 D.SessionOptions {id:session;executeActions:false}
 D.RemoteDesktopWindow {id:remote;shell:stub}
 D.TextToolsWindow {id:tools;shell:stub}
 D.FileInfoWindow {id:info}
 D.QuickLookWindow {id:preview}
 IpcHandler {target:"audit"
  function open(name:string):void {
   for(const window of [help,session,remote,tools,info,preview])window.visible=false;
   if(name==="help")help.show();
   else if(name==="session")session.show("suspend");
   else if(name==="remote")remote.visible=true;
   else if(name==="text")tools.visible=true;
   else if(name==="info")info.show({name:"Fixture.txt",path:"/fixture/Fixture.txt",size:"42 bytes",type:"Text",permissions:"Read and write"});
   else if(name==="quick"){preview.path="Fixture.txt";preview.result={type:"text",title:"Fixture.txt",text:"Stable preview content"};preview.visible=true;}
  }
  function state():string {return JSON.stringify({dark:D.Theme.dark,background:D.Theme.colors.background});}
 }
}''')
processes=[]
def launch(command,name):
    with (out/name).open('w') as _fixture_log0:
        process=sp.Popen(command,env=env,stdout=_fixture_log0,stderr=sp.STDOUT,start_new_session=True);processes.append(process);return process

def call(*command):return sp.check_output(command,env=env,text=True,timeout=10).strip()
def wait(fn):
    deadline=time.monotonic()+20
    while time.monotonic()<deadline:
        try:
            value=fn()
            if value:return value
        except (OSError,ValueError,sp.SubprocessError):pass
        time.sleep(.1)
    raise AssertionError('Live palette update timed out; inspect fixture logs')

def nodes(node):
    yield node
    for child in node.get('nodes',[])+node.get('floating_nodes',[]):yield from nodes(child)
def window(title):return next((node for node in nodes(json.loads(call('swaymsg','-r','-t','get_tree'))) if node.get('app_id')=='org.quickshell' and node.get('name')==title and node.get('visible')),None)
def ipc(*values):return call('quickshell','-p',str(qml),'ipc','call','audit',*values)
try:
    config=out/'sway.conf';config.write_text('output HEADLESS-1 resolution 1280x900\nxwayland disable\nfor_window [all] floating enable, border pixel 0\n')
    launch([a.sway,'--unsupported-gpu','-c',str(config)],'sway.log');runtime=Path(env['XDG_RUNTIME_DIR'])
    env['SWAYSOCK']=str(wait(lambda:next(runtime.glob('sway-ipc*.sock'),None)));env['WAYLAND_DISPLAY']=wait(lambda:next((path.name for path in runtime.glob('wayland-*') if not path.name.endswith('.lock')),None))
    launch(['quickshell','-p',str(qml)],'shell.log');wait(lambda:json.loads(ipc('state'))['background'])
    from PIL import Image
    results=[]
    for name,title in [('help','Desktop Help'),('session','Session Options'),('remote','Remote Desktop'),('text','Text Tools'),('info','File Info'),('quick','Quick Look')]:
        ipc('open',name);original=wait(lambda:window(title));identity=original['id'];rectangle=original['rect']
        for step,mode in enumerate(('light','dark','light')):
            colors=theme(mode);wait(lambda:json.loads(ipc('state'))['dark']==(mode=='dark'))
            expected={tuple(int(color[index:index+2],16) for index in (1,3,5)) for key,color in colors.items() if key in ('background','surface','field','hover')}
            old=palettes['dark' if mode=='light' else 'light'];old_colors={tuple(int(color[index:index+2],16) for index in (1,3,5)) for key,color in old.items() if key in ('background','surface','field','hover')}-expected
            def painted():
                current=window(title);assert current and current['id']==identity and current['rect']==rectangle,'Theme update recreated or moved the window'
                path=out/f'{name}-{step}-{mode}.png';call('grim',str(path));image=Image.open(path).convert('RGB');r=current['rect'];crop=image.crop((r['x']+10,r['y']+10,r['x']+r['width']-10,r['y']+r['height']-10));counts=Counter(crop.get_flattened_data());total=crop.width*crop.height
                fresh=sum(counts[color] for color in expected)/total;stale=sum(counts[color] for color in old_colors)/total
                if fresh<.35 or stale>.02:return None
                return dict(surface=name,mode=mode,step=step,windowId=identity,rect=rectangle,palettePixelFraction=fresh,stalePalettePixelFraction=stale)
            results.append(wait(painted))
    log=(out/'shell.log').read_text();assert not any(marker in log for marker in ('ReferenceError:','TypeError:','Binding loop','Failed to load configuration','Cannot assign')),log
    (out/'result.json').write_text(json.dumps(dict(passed=True,scope='Six owned windows kept mapped during light/dark/light atomic file updates; unchanged native identity and geometry, painted palette freshness',checks=results),indent=2)+'\n')
finally:
    for process in reversed(processes):
        if process.poll() is None:os.killpg(process.pid,signal.SIGTERM)
    for process in reversed(processes):
        try:process.wait(timeout=4)
        except sp.TimeoutExpired:os.killpg(process.pid,signal.SIGKILL)
print('PASS live appearance switching on six mapped owned windows')

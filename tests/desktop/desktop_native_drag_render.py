"""Production desktop native offers, confirmed incoming copy, and cancellation."""
from fixture_resources import close_reaped_pipes
import argparse,json,os,re,shutil,signal,subprocess as sp,sys,tempfile,time
from pathlib import Path
parser=argparse.ArgumentParser();parser.add_argument('--output',type=Path,required=True);parser.add_argument('--sway',default='sway');parser.add_argument('--pointer-tool',required=True);parser.add_argument('--skip-native-escape',action='store_true');parser.add_argument('--inside',action='store_true');args=parser.parse_args()
if not args.inside:
    private=tempfile.mkdtemp(prefix='desktop-native-drag-');env=dict(os.environ,HOME=private,XDG_RUNTIME_DIR=private)
    for key in ('DISPLAY','WAYLAND_DISPLAY','SWAYSOCK','DBUS_SESSION_BUS_ADDRESS'):env.pop(key,None)
    try:raise SystemExit(sp.call(['dbus-run-session','--',sys.executable,__file__,*sys.argv[1:],'--inside'],env=env))
    finally:shutil.rmtree(private,ignore_errors=True)
run=args.output.resolve();run.mkdir(parents=True,exist_ok=False);runtime=Path(os.environ['XDG_RUNTIME_DIR']);assets=Path(__file__).resolve().parents[2]/'res/lumen-desktop/shell';launcher=Path(shutil.which('lumen-desktop-shell'));prefix=re.split(r'\n[ \t]*exec quickshell ',launcher.read_text(),maxsplit=1)[0];raw=sp.check_output(['bash','-c',prefix+'\nexec env -0']);installed=dict(value.decode().split('=',1) for value in raw.split(b'\0') if b'=' in value);env=dict(os.environ)
for key in ('PATH','GLOBAL_MENU_PYTHON','QT_PLUGIN_PATH','QML_IMPORT_PATH'):
    if key in installed:env[key]=installed[key]
for kind in ('CONFIG','STATE','DATA','CACHE'):env['XDG_'+kind+'_HOME']=str(run/kind.lower())
env.update(WLR_BACKENDS='headless',WLR_RENDERER='pixman',WLR_LIBINPUT_NO_DEVICES='1',QT_QPA_PLATFORM='wayland',QT_QUICK_BACKEND='software',QT_QPA_PLATFORMTHEME='generic',QT_ACCESSIBILITY='0',NO_AT_BRIDGE='1',GTK_A11Y='none',LUMEN_FILE_SERVICE='0',LUMEN_LAUNCH_SCOPES='0')
for key in ('DISPLAY','WAYLAND_DISPLAY','SWAYSOCK'):env.pop(key,None)
directory=runtime/'Desktop';directory.mkdir();folder=directory/'Folder';folder.mkdir();document=directory/'document.txt';document.write_text('desktop original');incoming=runtime/'Incoming';incoming.mkdir();collision=incoming/'collision.txt';collision.write_text('source collision');fresh=incoming/'fresh.txt';fresh.write_text('source fresh');(folder/'collision.txt').write_text('existing destination')
configDirectory=Path(env['XDG_CONFIG_HOME']);configDirectory.mkdir();(configDirectory/'user-dirs.dirs').write_text('XDG_DESKTOP_DIR="$HOME/Desktop"\n')
palettes=json.loads((assets.parent/'appearance/palette.json').read_text());appearance=run/'appearance.json';appearance.write_text(json.dumps(dict(font=palettes['font'],palette=palettes['dark'],preferences={},desktopPreferences={'showFiles':True,'fileSort':'name','fileDescending':False})));env['LUMEN_APPEARANCE_STATE']=str(appearance)
desktop=run/'desktop.qml';desktop.write_text('''import QtQuick
import QtQuick.Window
import Quickshell
import Quickshell.Io
import Quickshell.Wayland
import "file:'''+str(assets)+'''" as D
ShellRoot {
 id:fixtureRoot
 property int backgroundClicks:0
 PanelWindow {anchors {left:true;right:true;top:true;bottom:true}exclusiveZone:0;WlrLayershell.layer:WlrLayer.Bottom;WlrLayershell.keyboardFocus:WlrKeyboardFocus.None;color:"#20242a";MouseArea {anchors.fill:parent;onClicked:fixtureRoot.backgroundClicks++;}}
 QtObject {id:quick;function show(path,owner){}}
 D.NativeFileDragState {id:dragFacts}
 QtObject {id:stub;property var quickLookWindow:quick;property bool nativeFileDragActive:dragFacts.active;property bool nativeFileDragAvailable:dragFacts.available;function openFiles(path){}}
 D.DesktopFilesSurface {id:surface;screen:Quickshell.screens[0];outputName:"HEADLESS-1";shell:stub;showFiles:true}
 IpcHandler {target:"dragTest"
  function status():string{return JSON.stringify({backgroundClicks:fixtureRoot.backgroundClicks,mask:surface.content.inputRectangles,count:surface.content.service.rows.length,active:surface.content.Window.window.active,native:surface.content.nativeDragActive,external:surface.content.externalDragActive,layout:surface.content.desktopLayout,dropPrompt:surface.content.dropConfirmationVisible,pending:surface.content.pendingDropPaths,error:surface.content.error || surface.content.service.error});}
  function confirm():void{surface.content.confirmDrop();}
  function selectAll():void{surface.content.selectedPaths=surface.content.service.rows.map(row=>row.path);}
 }
}
''')
peer=run/'peer.qml';peer.write_text('''import QtQuick
import Quickshell
import Quickshell.Io
ShellRoot {
 id:root
 property var received:[]
 property int finished:0
 property int action:0
 FloatingWindow {visible:true;title:"NativeFilePeer";implicitWidth:600;implicitHeight:500;color:"#dde4ee"
  DropArea {anchors.fill:parent;onEntered:drag=>{drag.accepted=drag.hasUrls;};onDropped:drop=>{if(drop.hasUrls){root.received=drop.urls.map(url=>url.toString());drop.accept(Qt.CopyAction);}}}
  Rectangle {id:offer;x:50;y:50;width:200;height:100;color:"#4080cc"
   Text {anchors.centerIn:parent;text:"Drag two incoming files";color:"white"}
   Drag.active:mouse.drag.active;Drag.dragType:Drag.Automatic;Drag.supportedActions:Qt.CopyAction;Drag.proposedAction:Qt.CopyAction
   Drag.mimeData:({"text/uri-list":"'''+collision.as_uri()+'''\\r\\n'''+fresh.as_uri()+'''\\r\\n"})
   Drag.onDragFinished:dropAction=>{root.finished++;root.action=dropAction;offer.x=50;offer.y=50;}
   MouseArea {id:mouse;anchors.fill:parent;drag.target:offer;drag.threshold:8}
  }
 }
 IpcHandler {target:"dragTest";function status():string{return JSON.stringify({received:root.received,finished:root.finished,action:root.action});}function clear():void{root.received=[];}}
}
''')
processes=[]
def launch(command,name):
    with (run/(name+'.log')).open('w') as _fixture_log0:
        process=sp.Popen(command,env=env,stdout=_fixture_log0,stderr=sp.STDOUT,start_new_session=True);processes.append(process);return process
def call(*command):return sp.check_output(command,env=env,text=True,timeout=10)
def ipc(path,method):return call('quickshell','-p',str(path),'ipc','call','dragTest',method)
def state(path):return json.loads(ipc(path,'status'))
def wait(predicate):
    deadline=time.monotonic()+20
    while time.monotonic()<deadline:
        try:
            if predicate():return
        except (ValueError,OSError,sp.SubprocessError):pass
        time.sleep(.1)
    raise AssertionError('Desktop native DnD timeout; inspect '+str(run))
def gesture(x,y,dx,dy,cancel=False):
    call('swaymsg','seat','seat0','cursor','set',str(x),str(y));pointer.stdin.write('m 1 1\n');pointer.stdin.flush();time.sleep(.15);pointer.stdin.write('p\n');pointer.stdin.flush();time.sleep(.15);pointer.stdin.write('m 30 20\n');pointer.stdin.flush();time.sleep(.15);pointer.stdin.write('m '+str(dx)+' '+str(dy)+'\n');pointer.stdin.flush();time.sleep(.4)
    (run/'last-gesture.json').write_text(json.dumps({'desktop':state(desktop),'peer':state(peer),'seats':json.loads(call('swaymsg','-t','get_seats')),'version':json.loads(call('swaymsg','-t','get_version'))},indent=2))
    if cancel:
        (run/'before-cancel.json').write_text(json.dumps({'desktop':state(desktop),'peer':state(peer),'tree':json.loads(call('swaymsg','-t','get_tree'))},indent=2));call('wtype','-k','Escape');time.sleep(.2)
    pointer.stdin.write('r\n');pointer.stdin.flush();time.sleep(.3)
try:
    config=run/'sway.conf';config.write_text('output HEADLESS-1 resolution 1440x1000\nxwayland disable\nseat seat0 fallback true\n')
    launch([args.sway,'--unsupported-gpu','-c',str(config)],'sway');wait(lambda:list(runtime.glob('sway-ipc*.sock')))
    env['SWAYSOCK']=str(next(runtime.glob('sway-ipc*.sock')));env['WAYLAND_DISPLAY']=next(path.name for path in runtime.glob('wayland-*') if not path.name.endswith('.lock'))
    pointer=sp.Popen([args.pointer_tool,'pointer','move','0','0'],env=env,stdin=sp.PIPE,text=True,start_new_session=True);processes.append(pointer);processes.append(sp.Popen(['wtype','-s','60000'],env=env,start_new_session=True));time.sleep(.3)
    launch(['quickshell','-p',str(desktop)],'desktop');wait(lambda:state(desktop)['count']==2)
    launch(['quickshell','-p',str(peer)],'peer');wait(lambda:state(peer)['received']==[]);call('swaymsg','[title="NativeFilePeer"] floating enable, resize set 720 960, move position 0 0');time.sleep(.3)
    # Public IPC tick hints cannot forge compositor-owned drag state.
    call('swaymsg','-t','send_tick',json.dumps({'kind':'lumen-drag','version':1,'seat':'seat0','active':True,'serial':999999}))
    time.sleep(.2);assert not state(desktop)['external'] and len(state(desktop)['mask'])==2
    originalLayout=state(desktop)['layout']
    # Escape cancels an actual desktop native offer without geometry/file writes.
    if not args.skip_native_escape:
        gesture(1360,180,-800,150,cancel=True);wait(lambda:not state(desktop)['native']);assert state(desktop)['layout']==originalLayout;assert state(peer)['received']==[];assert document.read_text()=='desktop original'
    # Production layer source -> separate normal client, actual compositor offer.
    gesture(1360,180,-800,150);wait(lambda:state(peer)['received']==[document.as_uri()]);assert state(desktop)['layout']==originalLayout;assert document.read_text()=='desktop original'
    # Multi-item offers retain all selected originals, including folders.
    ipc(peer,'clear');ipc(desktop,'selectAll');gesture(1360,180,-800,150);wait(lambda:len(state(peer)['received'])==2);assert set(state(peer)['received'])=={document.as_uri(),folder.as_uri()};assert state(desktop)['layout']==originalLayout
    # Normal client -> production desktop folder. Confirmation gates all copies.
    gesture(150,120,1180,-60);wait(lambda:state(desktop)['dropPrompt']);assert not (folder/'fresh.txt').exists();assert (folder/'collision.txt').read_text()=='existing destination';assert collision.read_text()=='source collision'
    call('grim',str(run/'incoming-confirmation.png'));call('wtype','-k','Escape');wait(lambda:not state(desktop)['dropPrompt']);assert not (folder/'fresh.txt').exists()
    time.sleep(.7);gesture(150,120,1180,-60);wait(lambda:state(desktop)['dropPrompt']);ipc(desktop,'confirm');wait(lambda:(folder/'fresh.txt').exists());assert (folder/'fresh.txt').read_text()=='source fresh';assert (folder/'collision.txt').read_text()=='existing destination';assert fresh.read_text()=='source fresh' and collision.read_text()=='source collision';assert state(peer)['action']==1
    # Native compositor facts temporarily expand empty desktop input regions.
    time.sleep(.7);gesture(150,120,1000,550);call('grim',str(run/'background-drop.png'));wait(lambda:state(desktop)['dropPrompt']);assert not (directory/'fresh.txt').exists();call('wtype','-k','Escape');wait(lambda:not state(desktop)['dropPrompt']);assert not (directory/'fresh.txt').exists()
    wait(lambda:not state(desktop)['external'] and len(state(desktop)['mask'])==2)
    clicks=state(desktop)['backgroundClicks'];call('swaymsg','seat','seat0','cursor','set','1100','750');pointer.stdin.write('m 1 1\np\nr\n');pointer.stdin.flush();wait(lambda:state(desktop)['backgroundClicks']==clicks+1)
    assert len(state(desktop)['mask'])==2
    for name in ('desktop','peer'):
        log=(run/(name+'.log')).read_text();assert not any(message in log for message in ('TypeError:','ReferenceError:','Failed to load')),log
    result={'passed':True,'checks':['forged tick cannot activate or wedge desktop drag mask','production desktop native offer to separate Qt client','external native offer to desktop folder','confirmation gates copying','Escape cancels incoming copy','confirmed copy creates fresh file','name collisions skipped without overwrite','source originals preserved','source accepted CopyAction','multi-item offer preserves originals','incoming background offer gets cancellable confirmation','first post-drop empty click reaches underlying background']};
    if not args.skip_native_escape:result['checks'].append('Escape cancels native offer and preserves metadata/files')
    (run/'result.json').write_text(json.dumps(result,indent=2)+'\n');print(json.dumps(result))
finally:
    for process in reversed(processes):
        if process.poll() is None:
            os.killpg(process.pid,signal.SIGTERM)
            try:process.wait(timeout=3)
            except sp.TimeoutExpired:os.killpg(process.pid,signal.SIGKILL);process.wait()
    for _fixture_process in processes:
        if _fixture_process.poll() is None:_fixture_process.wait(timeout=5)
    close_reaped_pipes(processes)

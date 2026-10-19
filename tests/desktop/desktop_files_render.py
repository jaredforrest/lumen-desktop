"""Disposable nested desktop files: real directory events and keyboard confirmation."""
import argparse,json,os,re,shutil,signal,subprocess as sp,sys,tempfile,time
from pathlib import Path
parser=argparse.ArgumentParser();parser.add_argument('--output',type=Path,required=True);parser.add_argument('--sway',default='sway');parser.add_argument('--pointer-tool',required=True);parser.add_argument('--layer',choices=['bottom','top'],default='bottom');parser.add_argument('--inside',action='store_true');args=parser.parse_args()
if not args.inside:
    runtime=tempfile.mkdtemp(prefix='desktop-files-')
    env=dict(os.environ,HOME=runtime,XDG_RUNTIME_DIR=runtime)
    for key in ('WAYLAND_DISPLAY','DISPLAY','SWAYSOCK','DBUS_SESSION_BUS_ADDRESS','AT_SPI_BUS_ADDRESS'):env.pop(key,None)
    try:raise SystemExit(sp.call(['dbus-run-session','--',sys.executable,__file__,*sys.argv[1:],'--inside'],env=env))
    finally:shutil.rmtree(runtime,ignore_errors=True)
run=args.output.resolve();run.mkdir(parents=True,exist_ok=False);runtime=Path(os.environ['XDG_RUNTIME_DIR'])
assets=Path(__file__).resolve().parents[2]/'res/lumen-desktop/shell'
launcher=Path(shutil.which('lumen-desktop-shell'));prefix=re.split(r'\n[ \t]*exec quickshell ',launcher.read_text(),maxsplit=1)[0]
raw=sp.check_output(['bash','-c',prefix+'\nexec env -0']);installed=dict(v.decode().split('=',1) for v in raw.split(b'\0') if b'=' in v)
env=dict(os.environ)
for key in ('PATH','GLOBAL_MENU_PYTHON','QT_PLUGIN_PATH','QML_IMPORT_PATH'):
    if key in installed:env[key]=installed[key]
for kind in ('CONFIG','STATE','DATA','CACHE'):env['XDG_'+kind+'_HOME']=str(run/kind.lower())
env.update(WLR_BACKENDS='headless',WLR_RENDERER='pixman',WLR_LIBINPUT_NO_DEVICES='1',QT_QPA_PLATFORM='wayland',QT_QUICK_BACKEND='software',QT_QPA_PLATFORMTHEME='generic',QT_ACCESSIBILITY='0',QT_LINUX_ACCESSIBILITY_ALWAYS_ON='0',NO_AT_BRIDGE='1',GTK_A11Y='none',LUMEN_FILE_SERVICE='0',LUMEN_LAUNCH_SCOPES='0')
for key in ('WAYLAND_DISPLAY','DISPLAY','SWAYSOCK'):env.pop(key,None)
desktop=runtime/'Desktop';desktop.mkdir();(desktop/'Folder').mkdir();(desktop/'one.txt').write_text('Disposable desktop fixture')
config=Path(env['XDG_CONFIG_HOME']);config.mkdir();(config/'user-dirs.dirs').write_text('XDG_DESKTOP_DIR="$HOME/Desktop"\n')
palettes=json.loads((assets.parent/'appearance/palette.json').read_text());appearance=run/'appearance.json';appearance.write_text(json.dumps(dict(font=palettes['font'],palette=palettes['dark'],preferences={},desktopPreferences={'showFiles':True,'fileSort':'name','fileDescending':False})));env['LUMEN_APPEARANCE_STATE']=str(appearance)
qml=run/'fixture.qml';qml.write_text('''import QtQuick
import Quickshell
import Quickshell.Io
import Quickshell.Wayland
import "file:'''+str(assets)+'''" as D
ShellRoot {
 QtObject {id:quick;function show(path,owner){}}
 QtObject {id:stub;property var quickLookWindow:quick;function openFiles(path){}}
 D.DesktopFilesSurface {id:surface;screen:Quickshell.screens[0];outputName:"HEADLESS-1";shell:stub;showFiles:true;WlrLayershell.layer:WlrLayer.'''+('Top' if args.layer=='top' else 'Bottom')+'''}
 IpcHandler {target:"desktopFilesTest"
  function status():string {return JSON.stringify({count:surface.content.service.rows.length,path:surface.content.directory,selected:surface.content.selectedPaths,confirm:surface.content.confirmationVisible,error:surface.content.error || surface.content.service.error,keyboard:surface.content.keyboardActive,allowKeyboard:surface.allowKeyboard,geometry:{x:surface.content.inputArea.x,y:surface.content.inputArea.y,width:surface.content.inputArea.width,height:surface.content.inputArea.height},size:{width:surface.width,height:surface.height}});}
  function select(index:int):void {surface.content.choose(index,0);}
  function askTrash():void {surface.content.askTrash();}
  function confirmTrash():void {surface.content.confirmTrash();}
  function rename(name:string):void {surface.content.service.act("rename",{path:surface.content.currentPath,name:name});}
 }
}
''')
processes=[]
def launch(command,name):
    process=sp.Popen(command,env=env,stdout=(run/(name+'.log')).open('w'),stderr=sp.STDOUT,start_new_session=True);processes.append(process)
def call(*command):return sp.check_output(command,env=env,text=True,timeout=10)
def wait(predicate):
    deadline=time.monotonic()+20
    while time.monotonic()<deadline:
        try:
            if predicate():return
        except (ValueError,OSError,sp.SubprocessError):pass
        time.sleep(.1)
    raise AssertionError('Desktop files readiness timed out; inspect '+str(run))
def ipc(method,*values):return call('quickshell','-p',str(qml),'ipc','call','desktopFilesTest',method,*values)
def state():return json.loads(ipc('status'))
try:
    swayConfig=run/'sway.conf';swayConfig.write_text('output HEADLESS-1 resolution 1440x1000\nxwayland disable\nseat seat0 fallback true\n')
    launch([args.sway,'--unsupported-gpu','-c',str(swayConfig)],'sway');wait(lambda:list(runtime.glob('sway-ipc*.sock')))
    env['SWAYSOCK']=str(next(runtime.glob('sway-ipc*.sock')));env['WAYLAND_DISPLAY']=next(path.name for path in runtime.glob('wayland-*') if not path.name.endswith('.lock'))
    pointer=sp.Popen([args.pointer_tool,'pointer','move','0','0'],env=env,stdin=sp.PIPE,text=True,start_new_session=True);processes.append(pointer)
    keyboard=sp.Popen(['wtype','-s','30000'],env=env,start_new_session=True);processes.append(keyboard);time.sleep(.3)
    launch(['quickshell','-p',str(qml)],'shell');wait(lambda:state()['count']==2)
    assert state()['path']==str(desktop) and not state()['error'],state()
    call('grim',str(run/'desktop.png'))
    # The only cursor and keyboard commands target this private compositor.
    call('swaymsg','seat','seat0','cursor','set','1360','180')
    pointer.stdin.write('m 10 10\n');pointer.stdin.flush();time.sleep(.15);pointer.stdin.write('p\n');pointer.stdin.flush();time.sleep(.15);pointer.stdin.write('r\n');pointer.stdin.flush()
    time.sleep(.3);call('grim','-c',str(run/'clicked.png'));(run/'click-state.json').write_text(json.dumps({'state':state(),'seats':json.loads(call('swaymsg','-t','get_seats','-r')),'inputs':json.loads(call('swaymsg','-t','get_inputs','-r')),'pointer_exit':pointer.poll()},indent=2))
    wait(lambda:state()['selected']==[str(desktop/'one.txt')])
    call('wtype','-k','Delete');wait(lambda:state()['confirm'])
    assert (desktop/'one.txt').exists(),'Trash ran before confirmation'
    call('grim',str(run/'confirmation.png'));call('wtype','-k','Escape');wait(lambda:not state()['confirm'])
    assert (desktop/'one.txt').exists(),'Escape executed a destructive action'
    ipc('rename','renamed.txt');wait(lambda:(desktop/'renamed.txt').exists() and not (desktop/'one.txt').exists())
    (desktop/'new.txt').write_text('watcher fixture');wait(lambda:state()['count']==3)
    (desktop/'new.txt').unlink();wait(lambda:state()['count']==2)
    # Explicit confirmation only affects the selected disposable file.
    ipc('select','1');ipc('askTrash');assert (desktop/'renamed.txt').exists();ipc('confirmTrash');wait(lambda:not (desktop/'renamed.txt').exists())
    assert (desktop/'Folder').is_dir(),'Unselected desktop item changed'
    result=dict(passed=True,checks=['configured Desktop directory','real bottom-layer click focus','physical Delete and Escape confirmation','rename','live add/remove events','explicit confirmed Trash','unselected item preserved'])
    (run/'result.json').write_text(json.dumps(result,indent=2)+'\n');print(json.dumps(result))
finally:
    for process in reversed(processes):
        if process.poll() is None:
            os.killpg(process.pid,signal.SIGTERM)
            try:process.wait(timeout=5)
            except sp.TimeoutExpired:os.killpg(process.pid,signal.SIGKILL);process.wait()

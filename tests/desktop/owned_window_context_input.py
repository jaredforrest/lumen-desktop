"""Real Files main/utility association in a disposable native Qt session."""
import argparse,json,os,re,shutil,signal,subprocess as sp,sys,tempfile,time
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--output',type=Path,required=True);p.add_argument('--launcher',type=Path,required=True);p.add_argument('--sway',required=True);p.add_argument('--full-shell',action='store_true');p.add_argument('--inside',action='store_true');p.add_argument('--effects',action='store_true');a=p.parse_args()
if not a.inside:
 with tempfile.TemporaryDirectory(prefix='lumen-owned-window-') as private:
  env=dict(os.environ,HOME=private,XDG_RUNTIME_DIR=private,DBUS_SYSTEM_BUS_ADDRESS='unix:path='+str(Path(private)/'no-system-bus'))
  for key in ('CONFIG','STATE','CACHE','DATA'):env['XDG_'+key+'_HOME']=str(Path(private)/key.lower())
  for key in ('DISPLAY','WAYLAND_DISPLAY','SWAYSOCK','DBUS_SESSION_BUS_ADDRESS','AT_SPI_BUS_ADDRESS'):env.pop(key,None)
  bus_config=Path(private)/'bus.conf'
  # No activation directories: owned UI tests must never start portal/FUSE services.
  bus_config.write_text('<busconfig><type>session</type><listen>unix:tmpdir='+private+'</listen><auth>EXTERNAL</auth><policy context="default"><allow send_destination="*"/><allow receive_sender="*"/><allow own="*"/></policy></busconfig>')
  raise SystemExit(sp.call(['dbus-run-session','--config-file='+str(bus_config),'--',sys.executable,__file__,*sys.argv[1:],'--inside'],env=env))
repo=Path(__file__).resolve().parents[2];out=a.output.resolve();out.mkdir(parents=True,exist_ok=False)
prefix=re.split(r'\n[ \t]*exec quickshell ',a.launcher.read_text(),maxsplit=1)[0]
env=dict(value.decode().split('=',1) for value in sp.check_output(['bash','-c',prefix+'\nexec env -0']).split(b'\0') if b'=' in value)
env.update(WLR_BACKENDS='headless',WLR_RENDERER='gles2' if a.effects else 'pixman',WLR_RENDERER_FORCE_SOFTWARE='1' if a.effects else '0',WLR_RENDERER_ALLOW_SOFTWARE='1',LIBGL_ALWAYS_SOFTWARE='1',WLR_LIBINPUT_NO_DEVICES='1',QT_QPA_PLATFORM='wayland',QT_QPA_PLATFORMTHEME='generic',QT_QUICK_BACKEND='software',QT_ACCESSIBILITY='0',QT_LINUX_ACCESSIBILITY_ALWAYS_ON='0',NO_AT_BRIDGE='1',LUMEN_FILE_SERVICE='0',LUMEN_LAUNCH_SCOPES='0',QT_NO_XDG_DESKTOP_PORTAL='1',LUMEN_NATIVE_WINDOW_CONTROLS='0',LUMEN_NATIVE_WINDOW_ACTIONS='0')
for key in ('DISPLAY','WAYLAND_DISPLAY','SWAYSOCK','AT_SPI_BUS_ADDRESS'):env.pop(key,None)
activatable=sp.check_output(['dbus-send','--session','--print-reply','--dest=org.freedesktop.DBus','/org/freedesktop/DBus','org.freedesktop.DBus.ListActivatableNames'],env=env,text=True)
assert set(re.findall(r'string "([^"]+)"',activatable))<={'org.freedesktop.DBus'},activatable
folder=Path(env['HOME'])/'First';folder.mkdir();document=folder/'selected.txt';document.write_text('Synthetic document for main/utility ownership')
applications=Path(env['XDG_DATA_HOME'])/'applications';applications.mkdir(parents=True,exist_ok=True);(applications/'lumen-files.desktop').write_text('[Desktop Entry]\nType=Application\nName=Files\nExec=true\nIcon=folder\n')
shutil.copytree(repo/'res/lumen-desktop',out/'assets')
qml=out/'shell.qml';qml.write_text('''import QtQuick
import QtQuick.Window
import Quickshell
import Quickshell.Io
import "assets/shell" as D
ShellRoot {
 id:fixture
 D.OwnedWindowContext {id:context}
 QtObject {id:exported;property var state:({appName:"",title:"",windowId:0})}
 QtObject {id:stub;property int focusEpoch:0;property var windows:[];property var exportedMenu:exported;property var ownedContext:context;property var filesWindow:first;function activeFiles(window){filesWindow=window;}function focusWindow(window){}function focusOwnedWindow(title){Quickshell.execDetached(["swaymsg",'[title="^'+title+'$"] focus']);} }
 D.FilesState {id:firstService;persistViewPreferences:false;path:'''+json.dumps(str(folder))+''';viewMode:"list"}
 D.FilesState {id:secondService;persistViewPreferences:false;path:'''+json.dumps(str(folder))+''';viewMode:"list"}
 D.FilesWindow {id:first;shell:stub;service:firstService;visible:true}
 D.FilesWindow {id:second;shell:stub;service:secondService;windowNumber:2;visible:false}
 D.QuickLookWindow {id:preview}
 FloatingWindow {id:other;title:"Unrelated fixture";visible:false;implicitWidth:240;implicitHeight:160;Rectangle{id:otherBody;anchors.fill:parent;color:"#405060"}}
 Component.onCompleted:{firstService.navigate(firstService.path);secondService.navigate(secondService.path);}
 IpcHandler {target:"ownershipAudit"
  function state():string{return JSON.stringify({ready:firstService.ready && !firstService.busy && firstService.rows.length>0,main:context.mainWindow===first?"first":context.mainWindow===second?"second":"",key:context.keyWindow===first?"first":context.keyWindow===second?"second":context.keyWindow===preview?"preview":context.keyWindow===first.informationWindow?"info":(second && context.keyWindow===second.informationWindow)?"secondInfo":"",role:context.keyRole,app:context.applicationId,menuTarget:stub.filesWindow===first?"first":"second",firstActive:first.selectionActive,previewActive:preview.keyActive,infoActive:first.informationWindow.keyActive,otherActive:otherBody.Window.window?otherBody.Window.window.active:false,firstTabs:first.tabs.length,secondTabs:second.tabs.length});}
  function firstFocus():void{first.focusContents();}
  function previewFocus():void{preview.show('''+json.dumps(str(document))+''',first);}
  function infoFocus():void{first.informationWindow.show({name:"Synthetic document",path:'''+json.dumps(str(document))+'''});}
  function secondInfoFocus():void{second.informationWindow.show({name:"Second synthetic document"});}
  function otherFocus():void{other.visible=true;Qt.callLater(()=>otherBody.Window.window.requestActivate());}
  function secondFocus():void{second.openPath('''+json.dumps(str(folder))+''');}
  function commandNewTab():void{stub.filesWindow.newTab();}
  function closeFirst():void{first.visible=false;}
  function closePreview():void{preview.returnTo=null;preview.visible=false;}
 }
}''')
if a.full_shell:
 qml=out/'assets/shell/shell.qml'
 original=qml.read_text();head,tail=original.rsplit('}',1)
 standalone=(out/'shell.qml').read_text()
 handler=standalone[standalone.index(' IpcHandler {target:"ownershipAudit"'):standalone.rfind('}')]
 handler=handler.replace('second.tabs.length','second?second.tabs.length:1').replace('context.keyWindow===second?','(second && context.keyWindow===second)?').replace('context.mainWindow===second?','(second && context.mainWindow===second)?')
 handler=handler.replace('function commandNewTab():void{stub.filesWindow.newTab();}', 'function commandNewTab():void{root.filesAction("new-tab");}')
 handler=handler.replace('function secondFocus():void{second.openPath(', 'function secondFocus():void{root.newFilesWindow(')
 handler=handler.replace('function state():string{return JSON.stringify({', 'function state():string{return JSON.stringify({filesGroupTitles:root.windowsFor(DesktopEntries.byId("lumen-files")).map(w=>w.title),group:root.active && root.entryFor(root.active)?root.entryFor(root.active).id:"",')
 qml.write_text(head+'''
 property var context:ownedWindowContext
 property var first:primaryFiles
 property var firstService:primaryFilesState
 property var second:filesInstances.length?filesInstances[0].window:null
 property var preview:quickLook
 property var stub:root
 FloatingWindow {id:other;title:"Unrelated fixture";visible:false;implicitWidth:240;implicitHeight:160;Rectangle{id:otherBody;anchors.fill:parent;color:"#405060"}}
 Component.onCompleted:root.openFiles('''+json.dumps(str(folder))+');\n'+handler+'}'+tail)
processes=[]
def launch(cmd,log):
 with (out/log).open('w') as stream:proc=sp.Popen(cmd,env=env,stdout=stream,stderr=sp.STDOUT,start_new_session=True)
 processes.append(proc);return proc
def call(*cmd):return sp.check_output(cmd,env=env,text=True,timeout=5).strip()
def ipc(method):return call('quickshell','-p',str(qml),'ipc','call','ownershipAudit',method)
def state():
 value=json.loads(ipc('state'));(out/'last-state.json').write_text(json.dumps(value,indent=2));return value
def wait(predicate):
 deadline=time.monotonic()+15
 while time.monotonic()<deadline:
  try:
   value=predicate()
   if value:return value
  except (ValueError,OSError,sp.SubprocessError):pass
  time.sleep(.04)
 raise AssertionError('Owned context timeout: '+str(out)+'; compositor logs: '+ '\n'.join(path.name+': '+path.read_text() for path in out.glob('*sway*.log')))
try:
 config=out/'sway.conf';config.write_text('output HEADLESS-1 resolution 1280x900\nxwayland disable\nfocus_follows_mouse no\nfor_window [all] floating enable\n')
 launch([a.sway,'--unsupported-gpu','-c',str(config)],'sway.log');runtime=Path(env['XDG_RUNTIME_DIR']);env['SWAYSOCK']=str(wait(lambda:next(runtime.glob('sway-ipc*.sock'),None)));env['WAYLAND_DISPLAY']=wait(lambda:next((path.name for path in runtime.glob('wayland-*') if not path.name.endswith('.lock')),None))
 launch(['quickshell','-p',str(qml)],'shell.log');wait(lambda:state()['ready'])
 ipc('firstFocus');wait(lambda:state()['key']=='first');assert state()['main']=='first'
 ipc('previewFocus');wait(lambda:state()['previewActive'] and state()['key']=='preview');assert state()['main']=='first' and state()['role']=='utility'
 if a.full_shell:wait(lambda:state()['group']=='lumen-files')
 ipc('commandNewTab');wait(lambda:state()['firstTabs']==2);assert state()['secondTabs']==1
 ipc('infoFocus');wait(lambda:state()['infoActive'] and state()['key']=='info');assert state()['main']=='first' and state()['menuTarget']=='first'
 ipc('otherFocus');wait(lambda:state()['otherActive'] and not state()['key']);assert state()['main']=='first'
 if a.full_shell:
  grouped=state()['filesGroupTitles'];assert 'Quick Look' in grouped and 'File Info' in grouped,grouped
 ipc('secondFocus');wait(lambda:state()['key']=='second');assert state()['main']=='second'
 ipc('secondInfoFocus');wait(lambda:state()['key']=='secondInfo');assert state()['main']=='second' and state()['menuTarget']=='second'
 ipc('previewFocus');wait(lambda:state()['key']=='preview');assert state()['main']=='first' and state()['menuTarget']=='first'
 ipc('closeFirst');wait(lambda:not state()['main']);assert not state()['app']
 (out/'result.json').write_text(json.dumps(dict(passed=True,checks=['Owned Files main document reference stays independent of Quick Look and inspector key focus','Document New Tab command from key utility targets associated Files instance','Another application clears owned key recipient without discarding main association','Second Files document and returning utility choose explicit owning instance','Closing main invalidates association'],final=state()),indent=2)+'\n')
finally:
 for process in reversed(processes):
  if process.poll() is None:os.killpg(process.pid,signal.SIGTERM)
 for process in reversed(processes):
  try:process.wait(timeout=3)
  except sp.TimeoutExpired:os.killpg(process.pid,signal.SIGKILL);process.wait(timeout=3)
print('PASS owned main/utility context')

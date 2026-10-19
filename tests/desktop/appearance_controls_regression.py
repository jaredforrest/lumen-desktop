"""Private QML interaction fixtures. No real session actions, files or accounts."""
import argparse,json,os,re,shutil,signal,subprocess as sp,sys,tempfile,time
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--output',type=Path,required=True);p.add_argument('--assets',type=Path);p.add_argument('--inside',action='store_true');a=p.parse_args()
if not a.inside:raise SystemExit(sp.call(['dbus-run-session','--',sys.executable,__file__,*sys.argv[1:],'--inside']))
repo=Path(__file__).resolve().parents[2];run=a.output.resolve();run.mkdir(parents=True,exist_ok=False)
prefix=re.split(r'\n[ \t]*exec quickshell ',Path(shutil.which('lumen-desktop-shell')).read_text(),maxsplit=1)[0]
raw=sp.check_output(['bash','-c',prefix+'\nexec env -0']);installed=dict(x.decode().split('=',1) for x in raw.split(b'\0') if b'=' in x)
env=dict(os.environ);env.update({k:installed[k] for k in ('PATH','QT_PLUGIN_PATH','QML_IMPORT_PATH','GLOBAL_MENU_PYTHON') if k in installed})
runtime=Path(tempfile.mkdtemp(prefix='appearance-qa-'))
env.update(HOME=str(run/'home'),XDG_RUNTIME_DIR=str(runtime),XDG_CONFIG_HOME=str(run/'config'),XDG_STATE_HOME=str(run/'state'),XDG_DATA_HOME=str(run/'data'),XDG_CACHE_HOME=str(run/'cache'),WLR_BACKENDS='headless',WLR_RENDERER='pixman',WLR_LIBINPUT_NO_DEVICES='1',QT_QPA_PLATFORM='wayland',GDK_BACKEND='wayland',GLOBAL_MENU_ENABLED='0',LUMEN_FILE_SERVICE='0',LUMEN_LAUNCH_SCOPES='0',LUMEN_NATIVE_WINDOW_CONTROLS='0',LUMEN_UNIFIED_OWNED_HEADERS='1',LUMEN_NATIVE_WINDOW_ACTIONS='0')
env.update(QT_ACCESSIBILITY='0',QT_LINUX_ACCESSIBILITY_ALWAYS_ON='0',NO_AT_BRIDGE='1',GTK_A11Y='none')
env.pop('AT_SPI_BUS_ADDRESS',None)
for k in ('DISPLAY','WAYLAND_DISPLAY','SWAYSOCK'):env.pop(k,None)
fontfile=sp.check_output(['fc-match','Interlude Variable','-f','%{file}'],text=True).strip()
fontconfig=run/'fonts.conf';fontconfig.write_text('<fontconfig><include>/etc/fonts/fonts.conf</include><dir>'+str(Path(fontfile).parent)+'</dir></fontconfig>');env['FONTCONFIG_FILE']=str(fontconfig)
(run/'state/lumen-desktop/session').mkdir(parents=True);docs=run/'home/Downloads';docs.mkdir(parents=True)
for i in range(20):(docs/('file-'+str(i)+'.txt')).write_text('synthetic')
qml=run/'shell.qml';qml.write_text('''import QtQuick
import Quickshell
import Quickshell.Io
import Quickshell.Wayland
import "file:'''+str(a.assets or repo/'res/lumen-desktop/shell')+'''" as D
ShellRoot {
 id:root
 property bool displayVisible:false
 property bool mediaVisible:false
 component FakePlayer:QtObject {
  property string dbusName:"org.mpris.fixture.one"
  property string identity:"Fixture Player"
  property bool isPlaying:false
  property bool canSeek:true
  property bool positionSupported:true
  property bool lengthSupported:true
  property bool canControl:true
  property bool canGoPrevious:true
  property bool canGoNext:true
  property bool canTogglePlaying:true
  property real position:20
  property real length:180
  property string trackTitle:"A synthetic track"
  property string trackArtist:"Fixture Artist"
  property string trackArtUrl:""
  function previous(){}
  function next(){}
  function togglePlaying(){isPlaying=!isPlaying;}
 }
 FakePlayer {id:firstPlayer}
 FakePlayer {id:secondPlayer;dbusName:"org.mpris.fixture.two";identity:"Second Fixture Player"}
 property int clicks:0
 property string confirmedAction:""
 QtObject {id:stub;property string popupOwner:"";signal dismissPopups(string owner);function claimPopup(name){popupOwner=name;dismissPopups(name);}function openFiles(path){} }
 D.SessionOptions {id:session;executeActions:false;onConfirmed:action=>root.confirmedAction=action}
 D.HelpWindow {id:help;shell:stub}
 D.QuickLookWindow {id:preview}
 D.TextToolsWindow {id:tools;shell:stub}
 D.RemoteDesktopWindow {id:remote;shell:stub}
 D.FileInfoWindow {id:info}
 D.FolderStack {id:stack;shell:stub}
 PanelWindow {id:panel;visible:!session.visible && !help.visible && !stack.visible && !preview.visible && !info.visible && !tools.visible && !remote.visible;anchors {top:true;bottom:true;left:true;right:true} color:D.Theme.colors.background;WlrLayershell.keyboardFocus:WlrKeyboardFocus.Exclusive
  D.MediaControl {id:media;x:30;y:30;width:300;height:270;visible:root.mediaVisible;expanded:true;players:[firstPlayer,secondPlayer]}
  D.DisplaySettingsPane {id:display;x:30;y:30;width:600;visible:root.displayVisible}
  Column {visible:!root.displayVisible && !root.mediaVisible;x:30;y:40;width:500;spacing:15
   D.DialogButton {id:button;text:"Delete Fixture";destructive:true;onClicked:root.clicks++}
   D.DesktopComboBox {id:combo;width:280;model:["One","Two","Three"];currentIndex:1}
   D.DesktopScrollBar {id:scroll;size:0.2;height:100;orientation:Qt.Vertical}
   D.CreativeTools {id:creative;width:500;page:"Colours"}
  }
 }
 IpcHandler {target:"test"
  function remoteShow():void {remote.connections=[{id:"fixture",name:"Office Computer",host:"office.example.test",protocol:"RDP",printer:true},{id:"vnc",name:"Studio Computer",host:"studio.example.test",protocol:"VNC",printer:false}];remote.visible=true;}
  function remoteSelect():void {remote.choose(0);}
  function remoteClose():void {remote.visible=false;}
  function previewShow(path:string):void {preview.show(path,null);}
  function previewSave(path:string):void {preview.subjectPath=path;preview.saveSubject();}
  function previewClose():void {preview.visible=false;}
  function infoShow():void {info.show({name:"Example document.txt",kind:"Plain text",size:"1,024 bytes",location:"Documents",modified:"Today",owner:"Fixture User",permissions:"Read and write"});}
  function infoClose():void {info.visible=false;}
  function toolShow(page:string):void {tools.page=page;tools.visible=true;}
  function toolClose():void {tools.visible=false;}
  function displayShow():void {root.displayVisible=true;display.outputs=[{name:"TEST-1",label:"Retina Display",mode:"2560x1440@60Hz",scale:2,x:0,y:0,modes:["2560x1440@60Hz"]},{name:"TEST-2",label:"External Display",mode:"1920x1080@60Hz",scale:1,x:1280,y:0,modes:["1920x1080@60Hz"]}];}
  function mediaCheck():string {root.mediaVisible=true;media.selectedPlayer=secondPlayer.dbusName;if(media.player!==secondPlayer)throw Error("Media source selection");media.seekTo(90);if(secondPlayer.position!==90 || firstPlayer.position!==20)throw Error("Seek changed wrong source");media.seekTo(999);if(secondPlayer.position!==180)throw Error("Seek beyond track");secondPlayer.canSeek=false;media.seekTo(30);if(secondPlayer.position!==180)throw Error("Seek ignored source capability");return "PASS media source and seek isolation";}
  function mediaClose():void {root.mediaVisible=false;}
  function displayClose():void {root.displayVisible=false;}
  function theme(dark:bool):void {D.Theme.colors=dark?{background:"#252527",surface:"#333336",field:"#404044",text:"#f5f5f7",muted:"#b0b0b8",border:"#555559",hover:"#45454a",accent:"#0a84ff",selectedText:"#ffffff"}:{background:"#f2f2f4",surface:"#ffffff",field:"#e5e5e8",text:"#202023",muted:"#6b6b70",border:"#c8c8ce",hover:"#dedee3",accent:"#007aff",selectedText:"#ffffff"};}
  function focusButton():void {button.forceActiveFocus();}
  function focusCombo():void {combo.forceActiveFocus();combo.popup.open();}
  function state():string {return JSON.stringify({remoteSelection:remote.selectedId,clicks:root.clicks,index:combo.currentIndex,highlight:combo.highlightedIndex,popup:combo.popup.visible,destructive:button.destructive,scrollOpacity:scroll.opacity,sessionVisible:session.visible,confirmed:root.confirmedAction,helpMatches:help.matches.length,stackRows:stack.rows.length,stackTotal:stack.total});}
  function sessionShow(action:string):void {session.show(action);}
  function sessionConfirm():void {session.confirm();}
  function helpShow():void {help.show();}
  function helpClose():void {help.visible=false;}
  function stackShow(path:string):void {stack.show(path,"");}
  function stackClose():void {stack.visible=false;}
  function colour():string {creative.setColour("#80ff0000");if(Math.abs(creative.chosen.a-128/255)>0.01 || creative.chosen.r<0.99)throw Error("Alpha/RGB round trip");creative.setColour("#007aff");if(creative.chosen.a!==1)throw Error("Opaque colour reset");return "PASS colour alpha and RGB round trip";}
 }
}''')
procs=[]
def launch(cmd,name):
 f=(run/(name+'.log')).open('w');proc=sp.Popen(cmd,env=env,stdout=f,stderr=sp.STDOUT,start_new_session=True);procs.append(proc);return proc
def call(*cmd):return sp.check_output(cmd,env=env,text=True,timeout=15).strip()
def wait(fn):
 end=time.monotonic()+12
 while time.monotonic()<end:
  try:
   if fn():return
  except (sp.SubprocessError,OSError,ValueError):pass
  time.sleep(.15)
 raise AssertionError('Fixture timed out; see '+str(run))
def ipc(*cmd):return call('quickshell','-p',str(qml),'ipc','call','test',*cmd)
try:
 conf=run/'sway.conf';conf.write_text('output HEADLESS-1 resolution 1280x900\nxwayland disable\nfor_window [all] floating enable, border pixel 0\n')
 launch(['sway','--unsupported-gpu','-c',str(conf)],'sway');wait(lambda:list(runtime.glob('sway-ipc*.sock')))
 env['SWAYSOCK']=str(next(runtime.glob('sway-ipc*.sock')));env['WAYLAND_DISPLAY']=next(x.name for x in runtime.glob('wayland-*') if not x.name.endswith('.lock'))
 ui=launch(['quickshell','-p',str(qml)],'ui');time.sleep(1)
 assert ui.poll() is None,(run/'ui.log').read_text()
 wait(lambda:'clicks' in ipc('state'))
 def focused_key(method,*keys):
  proc=sp.Popen(['wtype','-s','400',*keys],env=env);time.sleep(.15);ipc(method);assert proc.wait(timeout=3)==0;time.sleep(.1)
 focused_key('focusButton','-k','space');assert json.loads(ipc('state'))['clicks']==1
 focused_key('focusCombo','-k','Down','-k','Return');assert json.loads(ipc('state'))['index']==2
 assert ipc('colour').startswith('PASS')
 ipc('sessionShow','reboot');time.sleep(.3);call('grim',str(run/'session-confirmation.png'));call('swaymsg','[title="Session Options"] focus');call('wtype','-s','200','-k','Escape');assert not json.loads(ipc('state'))['sessionVisible'];assert not json.loads(ipc('state'))['confirmed']
 ipc('sessionShow','reboot');time.sleep(.3);call('swaymsg','[title="Session Options"] focus');call('wtype','-s','200','-k','Return');assert not json.loads(ipc('state'))['sessionVisible'];assert not json.loads(ipc('state'))['confirmed']
 ipc('sessionShow','logout');ipc('sessionConfirm');assert json.loads(ipc('state'))['confirmed']=='logout'
 ipc('helpShow');time.sleep(.3);call('swaymsg','[title="Desktop Help"] focus');call('wtype','-s','400','remote keyboard');wait(lambda:json.loads(ipc('state'))['helpMatches']==1);call('grim',str(run/'help-search.png'));ipc('helpClose')
 ipc('stackShow',str(docs));wait(lambda:json.loads(ipc('state'))['stackTotal']==20);assert json.loads(ipc('state'))['stackRows']==14;call('grim',str(run/'downloads-stack.png'));ipc('stackClose')
 time.sleep(1.7);assert json.loads(ipc('state'))['scrollOpacity']==0,'Automatic scrollbar did not hide at rest'
 call('grim',str(run/'light.png'))
 from PIL import Image
 example=docs/'example.png';Image.new('RGB',(800,500),'#007aff').save(example)
 assert ipc('mediaCheck').startswith('PASS');call('grim',str(run/'media-expanded.png'));ipc('mediaClose')
 ipc('previewShow',str(example));time.sleep(.7);call('grim',str(run/'quick-look.png'));ipc('previewSave',str(example));time.sleep(.6);call('grim',str(run/'save-dialog.png'));call('wtype','-k','Escape');ipc('previewClose')
 ipc('remoteShow');time.sleep(.3);call('swaymsg','[title="Remote Desktop"] focus');focused_key('remoteSelect','-k','Right');assert json.loads(ipc('state'))['remoteSelection']=='vnc';call('grim',str(run/'remote-desktop.png'));ipc('remoteClose')
 ipc('infoShow');time.sleep(.3);call('grim',str(run/'file-info.png'));ipc('infoClose')
 for dark in (False,True):
  ipc('theme','true' if dark else 'false')
  for page in ('Fonts','Colours','Spelling','Shortcuts'):
   ipc('toolShow',page);time.sleep(.35);call('grim',str(run/(('dark-' if dark else 'light-')+page.lower()+'.png')));ipc('toolClose')
  ipc('displayShow');time.sleep(.3);call('grim',str(run/(('dark-' if dark else 'light-')+'displays.png')));ipc('displayClose')
 for scale in (1.5,2):
  call('swaymsg','output HEADLESS-1 resolution 1920x1080 scale '+str(scale));ipc('toolShow','Colours');time.sleep(.4);call('grim',str(run/('colours-scale-'+str(scale)+'.png')));ipc('toolClose')

 (run/'result.json').write_text(json.dumps(dict(passed=True,checks=['media source selection, bounded seek and capability checks','destructive button keyboard activation','combo keyboard selection commits highlighted row','colour RGB and alpha round trip','automatic scrollbar idle hide','session cancellation and whitelisted confirmation without execution','Help search via real keyboard','Downloads preview reports all twenty files and limits visible rows to fourteen','isolated preview, inspector, utilities and display fixtures in light/dark with 150/200 percent captures']),indent=2))
 logs=(run/'ui.log').read_text()
 assert not any(marker in logs for marker in ('ReferenceError:','TypeError:','Binding loop','Failed to load configuration','Unable to assign','Cannot assign [undefined]')),logs
 print(run/'result.json')
finally:
 for proc in reversed(procs):
  if proc.poll() is None:
   os.killpg(proc.pid,signal.SIGTERM)
   try:proc.wait(timeout=3)
   except sp.TimeoutExpired:os.killpg(proc.pid,signal.SIGKILL)
 shutil.rmtree(runtime,ignore_errors=True)

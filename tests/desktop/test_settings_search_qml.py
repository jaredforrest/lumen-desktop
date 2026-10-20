"""Actual production settings search controls and keyboard navigation in private Qt."""
import json,os,re,shutil,subprocess,sys,tempfile
from private_native_bus import start_private_bus
from fixture_resources import close_reaped_pipes
from pathlib import Path
repo=Path(__file__).resolve().parents[2]
launcher=os.environ.get('LUMEN_TEST_SHELL_LAUNCHER') or shutil.which('lumen-desktop-shell')
prefix=re.split(r'\n[ \t]*exec quickshell ',Path(launcher).read_text(),maxsplit=1)[0]
with tempfile.TemporaryDirectory(prefix='lumen-settings-search-') as directory:
 root=Path(directory);env=dict(os.environ,HOME=directory,XDG_RUNTIME_DIR=directory)
 for key in ('CONFIG','STATE','CACHE','DATA'):env['XDG_'+key+'_HOME']=str(root/key.lower())
 env.update(DBUS_SYSTEM_BUS_ADDRESS='unix:path='+directory+'/absent-system',PULSE_SERVER='unix:'+directory+'/absent-audio')
 for name in ('WAYLAND_DISPLAY','DISPLAY','SWAYSOCK','AT_SPI_BUS_ADDRESS'):env.pop(name,None)
 raw=subprocess.check_output(['bash','-c',prefix+'\nexec env -0'],env=env);env.update(dict(v.decode().split('=',1) for v in raw.split(b'\0') if b'=' in v))
 env.update(QT_QPA_PLATFORM='offscreen',QT_QUICK_BACKEND='software',QT_ACCESSIBILITY='0',QT_LINUX_ACCESSIBILITY_ALWAYS_ON='0',NO_AT_BRIDGE='1',GTK_A11Y='none',QT_QPA_PLATFORMTHEME='generic',GLOBAL_MENU_PYTHON=sys.executable)
 for name in ('WAYLAND_DISPLAY','DISPLAY','SWAYSOCK','AT_SPI_BUS_ADDRESS'):env.pop(name,None)
 shutil.copytree(repo/'res/lumen-desktop',root/'assets')
 palette=json.loads((root/'assets/appearance/palette.json').read_text());appearance=root/'appearance.json';appearance.write_text(json.dumps(dict(font=palette['font'],palette=palette['dark'],preferences={})));env['LUMEN_APPEARANCE_STATE']=str(appearance)
 qml=root/'shell.qml';qml.write_text('''import QtQuick
import Quickshell
import "assets/shell" as D
ShellRoot {
 QtObject {id:service;property bool available:true;property bool busy:false;property string error:"";property string warning:"";property var preferences:({mode:"dark"});property var desktopPreferences:({});property var windowsPreferences:({});property var notificationsPreferences:({preview:"always"});property var searchPreferences:({});property var appearance:({});property var system:({});property int writes:0;function refresh(){} function setPreference(k,v){writes++;} function setWindowPreference(k,v){writes++;} property var menuBarPreferences:({})}
 QtObject {id:audio;property bool available:false;property bool muted:false;property real volume:0;property var outputs:[];property var inputs:[];property var streams:[];property bool inputAvailable:false}
 QtObject {id:notifications;property bool available:false;property bool dnd:false;property var applications:[];property var history:[];property var policies:({});property string error:""}
 QtObject {id:searchState;property bool available:false;property bool scanning:false;property int indexed:0}
 QtObject {id:shortcuts;property bool available:true;property string error:"";property var bindings:[{enabled:true,action:"overview",chord:"Mod4+Tab",scope:"global",appId:""}]}
 QtObject {id:stub;property var audioState:audio;property var notifications:notifications;property var search:searchState;property var shortcuts:shortcuts;property var screenSaverPreferences:null;property string volume:""}
 D.SettingsWindow {id:editor;shell:stub;service:service}
 property int step:0
 function find(name,item){if(item.objectName===name)return item;for(const child of item.children || []){const found=find(name,child);if(found)return found;}return null;}
 function check(ok,message){if(!ok)throw Error(message);}
 Timer {interval:100;running:true;repeat:true;onTriggered:{try {
  const search=find("settings-search",editor.contentItem);
  if(step===0){editor.openPage("Appearance");step++;return;}
  if(step===1){check(search!==null,"missing search");search.text="Title bar double-click action";check(editor.searchResults[0].control==="title-bar-double-click-action","wrong exact result");search.accepted();step++;return;}
  if(step===2){check(editor.page==="Desktop & Dock" && editor.controlId==="title-bar-double-click-action","wrong destination");check(editor.revealedControl && editor.revealedControl.activeFocus,"missing exact control focus");const viewport=find("settings-content-scroll",editor.contentItem);const at=editor.revealedControl.mapToItem(viewport.contentItem,0,0);check(at.y>=viewport.contentY && at.y+editor.revealedControl.height<=viewport.contentY+viewport.height,"not revealed "+JSON.stringify({y:at.y,h:editor.revealedControl.height,top:viewport.contentY,height:viewport.height,content:viewport.contentHeight}));editor.navigate("Accessibility","reduce-motion");step++;return;}
  if(step===3){editor.back(-1);step++;return;}
  if(step===4){check(editor.controlId==="title-bar-double-click-action" && editor.revealedControl.activeFocus,"Back lost control");editor.visible=false;editor.openPage();step++;return;}
  if(step===5){check(editor.controlId==="title-bar-double-click-action" && editor.revealedControl.activeFocus,"Reopen lost control");search.text="Dock size";const captured=editor.searchResults[0];search.text="Reduce motion";editor.acceptSearch(captured,"Dock size");check(editor.controlId==="title-bar-double-click-action","stale query activated");search.text="No matching label";search.accepted();check(editor.controlId==="title-bar-double-click-action","empty match changed destination");search.text="Reduce motion";search.accepted();step++;return;}
  if(step===6){check(editor.controlId==="reduce-motion" && editor.revealedControl.activeFocus,"new search focus");editor.navigate("Accessibility","configured-shortcuts");step++;return;}
  check(editor.revealedControl && editor.revealedControl.objectName==="settings-configured-shortcuts","shortcut section missing");check(service.writes===0,"navigation changed preferences");console.log("SETTINGS_SEARCH_QML_PASS");Qt.quit();
 }catch(error){console.error("SETTINGS_SEARCH_QML_FAIL "+error);Qt.quit();}}}
}''')
 children=[]
 try:
  start_private_bus(env,root,children)
  child=subprocess.Popen(['quickshell','-p',str(qml)],env=env,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT);children.append(child)
  output,_=child.communicate(timeout=10)
  result=subprocess.CompletedProcess(child.args,child.returncode,output)
 finally:
  for child in reversed(children):
   if child.poll() is None:
    child.terminate()
    try:child.wait(timeout=3)
    except subprocess.TimeoutExpired:child.kill();child.wait(timeout=3)
  close_reaped_pipes(children)
 assert result.returncode==0 and 'SETTINGS_SEARCH_QML_PASS' in result.stdout and 'SETTINGS_SEARCH_QML_FAIL' not in result.stdout,result.stdout
 assert not any(marker in result.stdout for marker in ('FAIL!','ReferenceError:','TypeError:','Binding loop','Failed to load configuration','Cannot assign','Unable to assign')),result.stdout
 print('PASS actual Qt settings search signal path, focus/reveal, exact-control history/reopen and stale query refusal')

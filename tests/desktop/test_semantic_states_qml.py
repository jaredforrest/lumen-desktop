"""Exercise actual control states and textual semantics without system actions."""
import os,re,shutil,subprocess,tempfile
from pathlib import Path
repo=Path(__file__).resolve().parents[2]
launcher=os.environ.get('LUMEN_TEST_SHELL_LAUNCHER') or shutil.which('lumen-desktop-shell')
prefix=re.split(r'\n[ \t]*exec quickshell ',Path(launcher).read_text(),maxsplit=1)[0]
with tempfile.TemporaryDirectory(prefix='lumen-semantic-states-') as directory:
    env=dict(os.environ,HOME=directory,XDG_RUNTIME_DIR=directory)
    for key in ('CONFIG','STATE','CACHE','DATA'):env['XDG_'+key+'_HOME']=directory+'/'+key.lower()
    for key in ('WAYLAND_DISPLAY','DISPLAY','DBUS_SESSION_BUS_ADDRESS','AT_SPI_BUS_ADDRESS'):env.pop(key,None)
    exported=subprocess.check_output(['bash','-c',prefix+'\nexec env -0'],env=env)
    env.update(dict(value.decode().split('=',1) for value in exported.split(b'\0') if b'=' in value))
    env.update(QT_QPA_PLATFORM='offscreen',QT_QPA_PLATFORMTHEME='generic',QT_QUICK_BACKEND='software',QT_ACCESSIBILITY='0',QT_LINUX_ACCESSIBILITY_ALWAYS_ON='0',NO_AT_BRIDGE='1',GTK_A11Y='none')
    path=Path(directory)/'shell.qml'
    path.write_text('''import QtQuick
import Quickshell
import "file:'''+str(repo/'res/lumen-desktop/shell')+'''" as D
ShellRoot {
 D.SettingsToggle {id:toggle;text:"Fixture permission";checked:false}
 D.ControlTile {id:tile;label:"Fixture radio";toggle:true;unavailable:false;active:false;subtitle:active?"Connected":"Disconnected"}
 D.StatusMessage {id:status;text:"Fixture request failed";severity:"error"}
 D.DialogButton {id:remove;text:"Delete Fixture";destructive:true;enabled:false}
 Timer {interval:100;running:true;onTriggered:{try {
  if(toggle.checked || !toggle.checkable || toggle.Accessible.role!==Accessible.CheckBox)throw Error("Toggle semantics missing");
  toggle.checked=true;
  if(!toggle.checked)throw Error("Toggle state absent");
  tile.active=true;
  if(!tile.Accessible.checkable || !tile.Accessible.checked || !tile.Accessible.name.includes("Connected"))throw Error("Radio state conveyed only by colour");
  tile.unavailable=true;
  if(tile.activeFocusOnTab || !tile.Accessible.name.includes("unavailable"))throw Error("Unavailable state lacks semantic explanation");
  if(!status.Accessible.name.startsWith("Error: ") || status.Accessible.description!=="error")throw Error("Error is only a colour");
  status.severity="success";
  if(!status.Accessible.name.startsWith("Success: "))throw Error("Success is only a colour");
  status.severity="warning";
  if(!status.Accessible.name.startsWith("Warning: ") || status.Accessible.description!=="warning")throw Error("Warning is only a colour");
  if(remove.enabled || !remove.destructive || remove.Accessible.name!=="Delete Fixture")throw Error("Destructive/disabled action identity missing");
  remove.enabled=true;remove.primary=true;
  if(remove.actionColor!==D.Theme.errorText || remove.contentItem.color!==D.Theme.destructiveText)throw Error("Filled destructive action bypasses semantic colours");
  remove.primary=false;
  if(remove.contentItem.color!==D.Theme.errorText)throw Error("Outlined destructive action bypasses semantic colours");
  console.log("SEMANTIC_STATES_PASS");Qt.quit();
 }catch(error){console.error("SEMANTIC_STATES_FAIL "+error);Qt.quit();}}}
}''')
    result=subprocess.run(['quickshell','-p',str(path)],env=env,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,timeout=15)
    assert result.returncode==0 and 'SEMANTIC_STATES_PASS' in result.stdout,result.stdout
    assert not any(value in result.stdout for value in ('SEMANTIC_STATES_FAIL','Binding loop','ReferenceError:','TypeError:','Failed to load configuration')),result.stdout
print('PASS actual QML toggle, radio, unavailable, status and destructive semantics independent of colour')

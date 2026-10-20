"""Actual Qt confirmation model: captured tokens, stale replies and no path fallback."""
import os,shutil,subprocess,tempfile,unittest
from pathlib import Path
ROOT=Path(__file__).resolve().parents[2]

class TrashState(unittest.TestCase):
 def test_actual_qt_state_and_confirmations(self):
  with tempfile.TemporaryDirectory(prefix='lumen-trash-qt-') as temporary:
   root=Path(temporary);assets=ROOT/'res/lumen-desktop/shell'
   for name in ('TrashActionsState.qml','TrashConfirmation.qml','TrashResults.qml','DialogButton.qml','PanelLabel.qml'):shutil.copy(assets/name,root/name)
   (root/'qmldir').write_text('singleton Theme 1.0 Theme.qml\n'+''.join(name+' 1.0 '+name+'.qml\n' for name in ('TrashActionsState','TrashConfirmation','TrashResults','DialogButton','PanelLabel')))
   (root/'Theme.qml').write_text('pragma Singleton\nimport QtQuick\nQtObject {property string font:"sans-serif";property var colors:({text:"#111111",surface:"#eeeeee",border:"#777777",accent:"#0066bb",selectedText:"#ffffff",hover:"#dddddd"});property color outline:"#777777";property color errorText:"#aa0000";property color destructiveText:"white";property color destructiveFill:"#aa0000";function textSize(value){return value;}function textRole(role){return 13;}}')
   (root/'shell.qml').write_text('''import QtQuick
import QtQuick.Controls
import Quickshell
ShellRoot {
 property var sent:[]
 property int changes:0
 TrashActionsState {id:state;command:["false"];onChanged:changes++;function send(action,data){sent=sent.concat([{action:action,data:data}]);return ++state.sequence;}}
 ApplicationWindow {id:window;width:600;height:500;visible:true
  TrashConfirmation {id:confirmation;state:state}
  TrashResults {state:state}
 }
 Timer {interval:100;running:true;onTriggered:{try {
  // Replace transport delivery only; exercise production state and dialog objects.
  state.prepare("empty",[]);
  if(sent[0].action!=="trash-prepare" || "items" in sent[0].data)throw Error("empty capture accepted caller paths");
  const retired=state.prepareId;state.retire();
  state.receive(JSON.stringify({action:"trash-prepare",id:retired,token:"stale",names:["wrong"],count:1}));
  if(state.confirmation || sent[sent.length-1].action!=="trash-cancel")throw Error("late preparation became actionable");
  state.prepare("selected",[{path:"/synthetic/Trash/files/a",identity:{device:"3",inode:"1152921504606846977"}}]);
  const captured=state.prepareId;
  state.receive(JSON.stringify({action:"trash-prepare",id:captured,token:"captured-token",names:["Literal <name> é"],count:1}));
  if(!confirmation.visible || state.confirmation.token!=="captured-token")throw Error("confirmation not opened");
  state.confirm();
  const confirm=sent.find(item=>item.action==="trash-confirm");
  if(!confirm || JSON.stringify(confirm.data)!==JSON.stringify({token:"captured-token"}) || state.confirmation)throw Error("confirmation retargeted or retained");
  state.receive(JSON.stringify({action:"trash-confirm",busy:false,operations:[{state:"complete"}],history:[]}));
  if(changes!==1 || state.busy || state.waiting)throw Error("immediate completion did not refresh consumer");
  console.log("TRASH_QT_PASS");Qt.quit();
 }catch(error){console.error("TRASH_QT_FAIL "+error);Qt.quit();}}}
}''')
   env=dict(os.environ,HOME=str(root),XDG_RUNTIME_DIR=str(root),XDG_STATE_HOME=str(root),XDG_DATA_HOME=str(root),XDG_CONFIG_HOME=str(root),XDG_CACHE_HOME=str(root),QT_QPA_PLATFORM='offscreen',QT_QUICK_BACKEND='software',QT_QPA_PLATFORMTHEME='generic',QT_ACCESSIBILITY='0',NO_AT_BRIDGE='1',DBUS_SESSION_BUS_ADDRESS='unix:path='+str(root/'inert-session'),DBUS_SYSTEM_BUS_ADDRESS='unix:path='+str(root/'inert-system'))
   for key in ('WAYLAND_DISPLAY','DISPLAY','AT_SPI_BUS_ADDRESS'):env.pop(key,None)
   result=subprocess.run(['quickshell','-p',str(root)],env=env,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,timeout=15)
   self.assertEqual(result.returncode,0,result.stdout);self.assertIn('TRASH_QT_PASS',result.stdout)
   for error in ('TRASH_QT_FAIL','Failed to load','ReferenceError','TypeError','Unable to assign','Binding loop'):self.assertNotIn(error,result.stdout)

if __name__=='__main__':unittest.main()

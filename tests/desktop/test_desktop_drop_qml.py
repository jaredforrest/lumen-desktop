"""Incoming local copy offers cannot mutate before explicit confirmation."""
import os,re,shutil,subprocess,tempfile,unittest
from pathlib import Path
class DesktopDropQt(unittest.TestCase):
    def test_rejected_offers_and_confirmation_gate(self):
        assets=Path(__file__).resolve().parents[2]/'res/lumen-desktop/shell';launcher=Path(shutil.which('lumen-desktop-shell'));prefix=re.split(r'\n[ \t]*exec quickshell ',launcher.read_text(),maxsplit=1)[0];raw=subprocess.check_output(['bash','-c',prefix+'\nexec env -0']);env=dict(value.decode().split('=',1) for value in raw.split(b'\0') if b'=' in value)
        with tempfile.TemporaryDirectory() as temporary:
            env.update(HOME=temporary,XDG_RUNTIME_DIR=temporary,XDG_STATE_HOME=temporary,XDG_CONFIG_HOME=temporary,XDG_DATA_HOME=temporary,XDG_CACHE_HOME=temporary,QT_QPA_PLATFORM='offscreen',QT_QUICK_BACKEND='software',QT_QPA_PLATFORMTHEME='generic',QT_ACCESSIBILITY='0',NO_AT_BRIDGE='1',GTK_A11Y='none')
            for key in ['DISPLAY','WAYLAND_DISPLAY','SWAYSOCK']:env.pop(key,None)
            qml=Path(temporary)/'shell.qml';qml.write_text('''import QtQuick
import Quickshell
ShellRoot {
 property int ticks:0
 QtObject {id:quick;function show(path,owner){}}
 QtObject {id:shell;property var quickLookWindow:quick;function openFiles(path){}}
 QtObject {id:service;property var rows:[];property bool busy:false;property string error:"";property string warning:"";property var calls:[];signal informationReady(var details);function startJob(operation,paths,destination,conflict){calls=calls.concat([{operation:operation,paths:paths,destination:destination,conflict:conflict}]);}}
 FloatingWindow {visible:true;implicitWidth:1200;implicitHeight:900
  Loader {id:desktop;anchors.fill:parent;Component.onCompleted:setSource("file:'''+str(assets/'DesktopFiles.qml')+'''",{shell:shell,service:service,resolveDirectory:false,directory:"/fixture",active:false})}
 }
 function check(value,message){if(!value)throw Error(message);}
 Timer {interval:100;running:true;repeat:true;onTriggered:{try{
  if(++ticks>30)throw Error("Drop fixture timeout");if(!desktop.item)return;
  const item=desktop.item;
  check(!item.proposeCopy(["https://invalid.test/file"],"/fixture/Folder"),"Remote offer accepted");check(service.calls.length===0,"Rejected offer started job");
  check(item.proposeCopy(["file:///fixture/a%20b","file:///fixture/a%20b"],"/fixture/Folder"),"Local offer rejected");check(item.dropConfirmationVisible && item.pendingDropPaths.length===1,"Prompt missing or duplicates retained");check(item.error==="","Successful offer retained stale error");check(service.calls.length===0,"Copy started before confirmation");
  check(!item.proposeCopy(["file:///fixture/b"],"relative"),"Relative destination accepted");check(!item.dropConfirmationVisible && item.pendingDropPaths.length===0,"Rejected replacement retained actionable pending copy");
  item.confirmDrop();check(service.calls.length===0,"Cancelled offer remained actionable");
  check(item.proposeCopy(["file:///fixture/a%20b"],"/fixture/Folder"),"Replacement offer rejected");
  item.confirmDrop();check(service.calls.length===1,"Confirmed copy not submitted");const job=service.calls[0];check(job.operation==="copy" && job.conflict==="skip" && job.destination==="/fixture/Folder" && job.paths[0]==="/fixture/a b","Copy contract changed");check(!item.dropConfirmationVisible && item.pendingDropPaths.length===0,"Prompt state retained");
  console.log("DESKTOP_DROP_QT_PASS");Qt.quit();
 }catch(error){console.error("DESKTOP_DROP_QT_FAIL "+error);Qt.quit();}}}
}
''')
            result=subprocess.run(['dbus-run-session','--','quickshell','-p',str(qml)],env=env,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,timeout=15)
            self.assertIn('DESKTOP_DROP_QT_PASS',result.stdout,result.stdout)
            for message in ['DESKTOP_DROP_QT_FAIL','TypeError:','ReferenceError:','Failed to load']:self.assertNotIn(message,result.stdout,result.stdout)
if __name__=='__main__':unittest.main()

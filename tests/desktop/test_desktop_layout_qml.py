"""Real Qt/subprocess arrangement persistence using disposable directories."""
import json,os,re,shutil,subprocess,tempfile,unittest
from pathlib import Path
class DesktopLayoutQt(unittest.TestCase):
    def test_drag_metadata_grouping_rename_and_restart(self):
        assets=Path(__file__).resolve().parents[2]/'res/lumen-desktop/shell';launcher=Path(os.environ.get('LUMEN_TEST_SHELL_LAUNCHER') or shutil.which('lumen-desktop-shell'));prefix=re.split(r'\n[ \t]*exec quickshell ',launcher.read_text(),maxsplit=1)[0];raw=subprocess.check_output(['bash','-c',prefix+'\nexec env -0']);env=dict(value.decode().split('=',1) for value in raw.split(b'\0') if b'=' in value)
        with tempfile.TemporaryDirectory() as temporary:
            folder=Path(temporary)/'Desktop';folder.mkdir();(folder/'one.txt').write_text('unchanged')
            env.update(HOME=temporary,XDG_RUNTIME_DIR=temporary,XDG_STATE_HOME=temporary,XDG_CONFIG_HOME=temporary,XDG_DATA_HOME=temporary,XDG_CACHE_HOME=temporary,QT_QPA_PLATFORM='offscreen',QT_QUICK_BACKEND='software',QT_QPA_PLATFORMTHEME='generic',QT_ACCESSIBILITY='0',NO_AT_BRIDGE='1',GTK_A11Y='none')
            for key in ['DISPLAY','WAYLAND_DISPLAY','SWAYSOCK']:env.pop(key,None)
            qml=Path(temporary)/'shell.qml';qml.write_text('''import QtQuick
import Quickshell
ShellRoot {
 property int phase:0
 property int ticks:0
 Loader {id:state;Component.onCompleted:setSource("file:'''+str(assets/'DesktopLayoutState.qml')+'''",{directory:"'''+str(folder)+'''",rows:[{name:"one.txt",identity:{device:1,inode:2}}],active:true})}
 function check(value,message){if(!value)throw Error(message);}
 Timer {interval:70;running:true;repeat:true;onTriggered:{try {
  if(++ticks>120)throw Error("Desktop arrangement fixture timeout "+phase);
  if(!state.item || !state.item.ready || state.item.busy)return;
  const value=state.item;
  if(phase===0){value.move("1:2",196,120,500,400);phase=1;return;}
  if(phase===1){check(value.layout.positions["1:2"][0]===.5,"Drag normalization lost");value.rows=[{name:"renamed.txt",identity:{device:1,inode:2}}];phase=2;return;}
  if(phase===2){check(value.layout.positions["1:2"][1]===.4,"Rename discarded stable placement");value.group("kind");phase=3;return;}
  if(phase===3){state.active=false;phase=4;return;}
  if(phase===4)return;
  if(phase===5){check(value.layout.grouping==="kind" && value.layout.positions["1:2"][0]===.5,"Restart did not restore metadata");console.log("DESKTOP_LAYOUT_QT_PASS");Qt.quit();}
 }catch(error){console.error("DESKTOP_LAYOUT_QT_FAIL "+error);Qt.quit();}}}
 Timer {interval:80;running:true;repeat:true;onTriggered:if(phase===4){state.active=true;state.setSource("file:'''+str(assets/'DesktopLayoutState.qml')+'''",{directory:"'''+str(folder)+'''",rows:[{name:"renamed.txt",identity:{device:1,inode:2}}],active:true});phase=5;}}
}
''')
            result=subprocess.run(['dbus-run-session','--','quickshell','-p',str(qml)],env=env,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,timeout=20)
            self.assertIn('DESKTOP_LAYOUT_QT_PASS',result.stdout,result.stdout)
            for failure in ['DESKTOP_LAYOUT_QT_FAIL','Failed to load','ReferenceError:','TypeError:']:self.assertNotIn(failure,result.stdout,result.stdout)
            self.assertEqual((folder/'one.txt').read_text(),'unchanged');self.assertEqual(list(folder.iterdir()),[folder/'one.txt'])
if __name__=='__main__':unittest.main()

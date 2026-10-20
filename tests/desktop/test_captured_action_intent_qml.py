"""Actual Qt button intent capture; no provider or external application is started."""
import os,re,shutil,subprocess,tempfile,unittest
from pathlib import Path
class Intent(unittest.TestCase):
 def test_pressed_and_keyboard_intents_never_retarget(self):
  root=Path(__file__).resolve().parents[2];launcher=os.environ.get('LUMEN_TEST_SHELL_LAUNCHER') or shutil.which('lumen-desktop-shell');self.assertIsNotNone(launcher)
  prefix=re.split(r'\n[ \t]*exec quickshell ',Path(launcher).read_text(),maxsplit=1)[0]
  installed=dict(v.decode().split('=',1) for v in subprocess.check_output(['bash','-c',prefix+'\nexec env -0']).split(b'\0') if b'=' in v)
  with tempfile.TemporaryDirectory() as temporary:
   qml=Path(temporary)/'shell.qml';qml.write_text('''import QtQuick
import Quickshell
import "file:'''+str(root/'res/lumen-desktop/shell')+'''" as D
ShellRoot {
 id:root
 property var activated:[]
 D.CapturedActionButton {id:button;selection:({capability:"first",session:"provider-one"});onActivatedIntent:value=>root.activated=root.activated.concat([value])}
 function check(value,message){if(!value)throw new Error(message);}
 Timer {interval:1000;running:true;onTriggered:Qt.quit()}
 Component.onCompleted:{
  button.beginIntent();button.selection={capability:"second",session:"provider-one"};button.clicked();check(activated.length===0,"Held pointer retargeted new resource");
  button.beginIntent();button.selection={capability:"second",session:"provider-two"};button.clicked();check(activated.length===0,"Provider replacement retained old intent");
  button.beginIntent();button.cancelIntent();button.selection=null;button.clicked();check(activated.length===0,"Canceled intent activated");
  button.selection={capability:"second",session:"provider-two"};button.clicked();check(activated.length===1 && activated[0].capability==="second","Keyboard activation failed exact current capture");
  button.beginIntent();button.clicked();check(activated.length===2,"Exact pointer activation missing");
  button.selection={capability:"second",session:"provider-two",choice:"chooser",application:"first.desktop"};button.beginIntent();button.selection={capability:"second",session:"provider-two",choice:"chooser",application:"second.desktop"};button.clicked();check(activated.length===2,"Application delegate was retargeted during held press");
  console.log("PASS captured pointer, cancellation, keyboard and provider identities");Qt.quit();
 }
}''')
   env=dict(installed,HOME=temporary,XDG_RUNTIME_DIR=temporary,QT_QPA_PLATFORM='offscreen',QT_QPA_PLATFORMTHEME='',GTK_A11Y='none',QT_QUICK_BACKEND='software',QT_ACCESSIBILITY='0',NO_AT_BRIDGE='1');env.pop('WAYLAND_DISPLAY',None);env.pop('SWAYSOCK',None);env.pop('DISPLAY',None);env['DBUS_SESSION_BUS_ADDRESS']='unix:path='+temporary+'/no-bus'
   result=subprocess.run(['quickshell','-p',str(qml)],env=env,text=True,capture_output=True,timeout=15);self.assertEqual(result.returncode,0,result.stdout+result.stderr);self.assertIn('PASS captured pointer',result.stdout+result.stderr)
if __name__=='__main__':unittest.main()

"""Actual QML battery presentation clears on removal, invalid input and timeout."""
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile
import unittest


class BatteryPresentation(unittest.TestCase):
    def test_actual_state_and_accessible_button(self):
        source=Path(__file__).resolve().parents[2]/'res/lumen-desktop/shell'
        launcher=Path(os.environ.get('LUMEN_TEST_SHELL_LAUNCHER') or shutil.which('lumen-desktop-shell'))
        prefix=re.split(r'\n[ \t]*exec quickshell ',launcher.read_text(),maxsplit=1)[0]
        raw=subprocess.check_output(['bash','-c',prefix+'\nexec env -0'])
        env=dict(value.decode().split('=',1) for value in raw.split(b'\0') if b'=' in value)
        with tempfile.TemporaryDirectory() as temporary:
            directory=Path(temporary)
            for name in ('HOME','XDG_RUNTIME_DIR','XDG_CONFIG_HOME','XDG_STATE_HOME','XDG_DATA_HOME','XDG_CACHE_HOME'):
                env[name]=str(directory/name);Path(env[name]).mkdir(mode=0o700)
            for name in ('SWAYSOCK','DISPLAY','WAYLAND_DISPLAY','AT_SPI_BUS_ADDRESS'):env.pop(name,None)
            env.update(QT_QPA_PLATFORM='offscreen',QT_QPA_PLATFORMTHEME='generic',QT_QUICK_BACKEND='software',QT_ACCESSIBILITY='0',QT_LINUX_ACCESSIBILITY_ALWAYS_ON='0',NO_AT_BRIDGE='1',GTK_A11Y='none')
            qml=directory/'shell.qml'
            qml.write_text('''import QtQuick
import Quickshell
import "file:'''+str(source)+'''" as D
ShellRoot {
 property int tick:0
 D.BatteryState {id:battery;freshnessMilliseconds:50}
 FloatingWindow {visible:true;implicitWidth:180;implicitHeight:50
  D.BatteryMenuButton {id:indicator;power:battery.status}
 }
 function check(value,message){if(!value)throw Error(message);}
 Timer {interval:30;running:true;repeat:true;onTriggered:{
  try {
   ++tick;
   if(tick===1){
    check(!indicator.visible,"Unavailable battery was shown");
    battery.publish({available:true,present:true,percentage:42,state:"charging",secondsRemaining:1200});
    check(indicator.visible && indicator.Accessible.name.includes("42%") && indicator.Accessible.name.includes("Charging"),"Charging state missing");
    check(indicator.Accessible.description.includes("until full"),"Estimate inaccessible");
    battery.publish({available:true,present:true,percentage:4,state:"discharging",secondsRemaining:60});
    check(indicator.Accessible.name.includes("Low battery"),"Low state depended only on colour");
    battery.publish({available:true,present:false,percentage:99});
    check(!indicator.visible && battery.status.percentage===null,"Removed battery retained its percentage");
    battery.publish({available:true,present:true,percentage:101,state:"charging",secondsRemaining:null});
    check(!indicator.visible && !battery.status.available,"Malformed percentage accepted");
    battery.publish({available:true,present:true,percentage:0,state:"empty",secondsRemaining:null});
    check(indicator.visible && indicator.Accessible.name.includes("0%"),"Real zero charge hidden");
   }
   if(tick===4){
    check(!indicator.visible && battery.status.percentage===null,"Unresponsive provider retained stale charge");
    console.log("BATTERY_PRESENTATION_PASS");Qt.quit();
   }
  }catch(error){console.error("BATTERY_PRESENTATION_FAIL "+error);Qt.quit();}
 }}
}
''')
            result=subprocess.run(['dbus-run-session','--','quickshell','-p',str(qml)],env=env,text=True,capture_output=True,timeout=20)
            output=result.stdout+result.stderr
            self.assertEqual(result.returncode,0,output)
            self.assertIn('BATTERY_PRESENTATION_PASS',output)
            for problem in ('BATTERY_PRESENTATION_FAIL','TypeError:','ReferenceError:','is not a type','Error:'):
                self.assertNotIn(problem,output)


if __name__=='__main__':unittest.main()

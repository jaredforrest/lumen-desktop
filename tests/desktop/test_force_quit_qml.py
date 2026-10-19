"""A replacement confirmation cannot inherit an older application's token."""
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest

ROOT=Path(__file__).resolve().parents[2]

class ForceQuitQmlTests(unittest.TestCase):
    def test_queued_selection_cancel_and_explicit_confirmation(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary);fixture=root/'fixture';fixture.mkdir();runtime=root/'runtime';runtime.mkdir(mode=0o700)
            for name in ['ForceQuitDialog.qml','PanelSurface.qml','PanelLabel.qml','ControlRow.qml']:shutil.copy(ROOT/'res/lumen-desktop/shell'/name,fixture/name)
            # The fast unit fixture executes the original dialog state machine
            # in a plain item. Wayland surface loading is covered by shell smoke.
            (fixture/'ForceQuitDialog.qml').chmod(0o600)
            dialog=(fixture/'ForceQuitDialog.qml').read_text().replace('PanelWindow {','Rectangle {',1)
            dialog=dialog.replace('    anchors {top:true;bottom:true;left:true;right:true}','    property var screen:null\n    width:800;height:600',1)
            dialog='\n'.join(line for line in dialog.splitlines() if not line.strip().startswith(('exclusionMode:','WlrLayershell.')))
            (fixture/'ForceQuitDialog.qml').write_text(dialog)
            (fixture/'qmldir').write_text('singleton Theme 1.0 Theme.qml\nForceQuitDialog 1.0 ForceQuitDialog.qml\nPanelSurface 1.0 PanelSurface.qml\nPanelLabel 1.0 PanelLabel.qml\nControlRow 1.0 ControlRow.qml\n')
            (fixture/'Theme.qml').write_text('pragma Singleton\nimport QtQuick\nQtObject {property string font:"Sans";property color errorText:"#b42318";property string outline:"#999999";property string popoverMaterial:"#eeeeee";property bool dark:false;property var colors:({background:"#eeeeee",hover:"#dddddd",field:"#ffffff",accent:"#0088ff",text:"#000000",muted:"#666666"})}')
            helper=root/'lumen-application-control';log=root/'calls.jsonl'
            helper.write_text('#!'+sys.executable+'\nimport json,sys,time\nfrom pathlib import Path\nrequest=sys.argv[2];action=sys.argv[3];value=sys.argv[4]\nwith Path('+repr(str(log))+').open("a") as output:output.write(json.dumps(dict(request=request,action=action,value=value))+"\\n")\nif action=="prepare":\n if json.loads(value)[0]["title"]=="slow":time.sleep(.2)\n print(json.dumps(dict(requestId=request,token=format(int(request),"032x"),windowCount=1)))\nelse:print(json.dumps(dict(requestId=request,terminated=1,cancelled=True)))\n')
            helper.chmod(0o700)
            (fixture/'shell.qml').write_text('''import QtQuick
import Quickshell
import "."
ShellRoot {id:mock
 property string popupOwner:""
 signal dismissPopups(string owner)
 function preferredOutput(){return "";}
 function claimPopup(owner){popupOwner=owner;dismissPopups(owner);}
 ForceQuitDialog {id:dialog;shell:mock}
 Timer {property int stage:0;interval:30;running:true;repeat:true
  onTriggered:{try {
   if(stage===0){dialog.show("Old",[{appId:"old",title:"slow"}]);stage++;}
   else if(stage===1){dialog.show("New",[{appId:"new",title:"fast"}]);stage++;}
   else if(stage===2 && dialog.phase==="ready"){
    if(dialog.applicationName!=="New" || dialog.token!=="00000000000000000000000000000002")throw Error("Old token inherited");
    dialog.cancel();dialog.show("Final",[{appId:"final",title:"fast"}]);stage++;
   }else if(stage===3 && dialog.phase==="ready"){
    if(dialog.token!=="00000000000000000000000000000004")throw Error("Cancelled token inherited");
    dialog.confirm();stage++;
   }else if(stage===4 && dialog.phase==="closed") {console.log("PASS Force Quit selection identity and explicit confirmation");Qt.quit();}
   if(dialog.phase==="error")throw Error(dialog.error);
  }catch(error){console.error("FAIL "+error);Qt.quit();}}
 }
}''')
            env=dict(os.environ,PATH=str(root)+os.pathsep+os.environ['PATH'],HOME=str(root/'home'),XDG_RUNTIME_DIR=str(runtime),QT_QPA_PLATFORM='offscreen',QT_QPA_PLATFORMTHEME='',QT_QUICK_BACKEND='software',QT_ACCESSIBILITY='0',QT_LINUX_ACCESSIBILITY_ALWAYS_ON='0',NO_AT_BRIDGE='1',GTK_A11Y='none')
            for key in ['CONFIG','CACHE','STATE','DATA']:env['XDG_'+key+'_HOME']=str(root/key.lower())
            for key in ['WAYLAND_DISPLAY','DISPLAY','SWAYSOCK','AT_SPI_BUS_ADDRESS']:env.pop(key,None)
            result=subprocess.run(['quickshell','-p',str(fixture)],env=env,capture_output=True,text=True,timeout=10)
            self.assertEqual(result.returncode,0,result.stdout+result.stderr)
            self.assertIn('PASS Force Quit',result.stdout+result.stderr)
            self.assertNotIn('FAIL ',result.stdout+result.stderr)
            calls=[json.loads(line) for line in log.read_text().splitlines()]
            cancels=[call for call in calls if call['action']=='cancel']
            self.assertEqual([call['value'] for call in cancels],['00000000000000000000000000000002'])
            confirms=[call for call in calls if call['action']=='confirm']
            self.assertEqual(len(confirms),1,calls)
            self.assertEqual(confirms[0]['value'],'00000000000000000000000000000004')

if __name__=='__main__':unittest.main()

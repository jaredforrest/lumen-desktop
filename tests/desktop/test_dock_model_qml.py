"""Exercise the actual Dock model using Qt's JavaScript engine."""
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]

class DockModelTests(unittest.TestCase):
    def test_working_pins_running_extras_and_stable_tail(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary)
            shutil.copy(ROOT/'res/lumen-desktop/shell/DockModel.js',root/'DockModel.js')
            (root/'shell.qml').write_text('''import QtQuick
import Quickshell
import "DockModel.js" as DockModel
Timer {
 interval:100;running:true
 onTriggered:{try {
 const entries=[{id:"brave-browser",name:"Brave"},{id:"ai.storyteller.wordcraft",name:"WordCraft"},{id:"foot",name:"Terminal"}];
 const reference=[{key:"fake",appId:null,action:null},{key:"brave",appId:"brave-browser"},{key:"pages",appId:"writer"},{key:"files",action:"files"},{key:"downloads",action:"downloads"},{key:"trash",action:"trash"}];
 const model=DockModel.build(reference,entries,[entries[0],entries[2],entries[2]]);
 const keys=model.map(x=>x.key);
 if(JSON.stringify(keys)!==JSON.stringify(["brave","pages","files","foot","downloads","trash"])) throw Error("Invalid identity/order: "+JSON.stringify(keys));
 if(model[1].entry.id!=="ai.storyteller.wordcraft") throw Error("Office mapping lost");
 if(model.some((x,i)=>i>0 && x.center<=model[i-1].center))throw Error("Overlapping slots");
 const files={id:"lumen-files",name:"Files"};
 const withOwned=DockModel.build(reference,entries.concat([files]),[files]);
 if(withOwned.filter(x=>x.entry && x.entry.id==="lumen-files").length!==1)throw Error("Duplicate owned app");
 const empty=DockModel.build(reference,[],[]);
 if(empty.some(x=>!x.action))throw Error("Unassigned icon survived");
 console.log("PASS dynamic Dock identity, deduplication, working pins and terminal tail");
 } catch(e){console.error("FAIL "+e)}Qt.quit();}
}''')
            env=dict(os.environ,QT_QPA_PLATFORM='offscreen',XDG_CONFIG_HOME=str(root/'config'),XDG_CACHE_HOME=str(root/'cache'))
            env.pop('WAYLAND_DISPLAY',None)
            run=subprocess.run(['quickshell','-p',str(root)],env=env,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,timeout=10)
            self.assertEqual(run.returncode,0,run.stdout)
            self.assertIn('PASS dynamic Dock',run.stdout)
            self.assertNotIn('FAIL ',run.stdout)

if __name__=='__main__':unittest.main()

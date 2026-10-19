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
 const custom=DockModel.build(reference,entries,[entries[2]], ["ai.storyteller.wordcraft.desktop"], [entries[0],entries[1]], true);
 if(JSON.stringify(custom.map(x=>x.key))!==JSON.stringify(["files","pages","foot","brave-browser","downloads","trash"]))throw Error("Custom pin/running/recent order "+JSON.stringify(custom));
 if(custom[3].section!=="recent")throw Error("Recent section not identified");
 if(DockModel.build(reference,entries,[],[],[entries[0]],false).some(x=>x.entry))throw Error("Disabled recents survived");
 if(JSON.stringify(DockModel.updatePins(["a","b","c"],"b","earlier"))!==JSON.stringify(["b","a","c"]))throw Error("Earlier reorder failed");
 if(JSON.stringify(DockModel.updatePins(["a","b","c"],"b","later"))!==JSON.stringify(["a","c","b"]))throw Error("Later reorder failed");
 if(JSON.stringify(DockModel.updatePins(["a.desktop","b"],"a","remove"))!==JSON.stringify(["b"]))throw Error("Canonical pin removal failed");
 if(JSON.stringify(DockModel.movePin(["a","b","c"],"c","a"))!==JSON.stringify(["c","a","b"]))throw Error("Drag insertion failed");
 if(JSON.stringify(DockModel.movePin(["a","b"],"a","missing"))!==JSON.stringify(["a","b"]))throw Error("Invalid drag target changed pins");
 const stacked=DockModel.build(reference,entries,[],[],[],false,[{path:"/fixture/Reports",sort:"name",descending:false,presentation:"list"}]);
 if(stacked[1].action!=="folder" || stacked[1].folder.path!=="/fixture/Reports" || stacked[stacked.length-1].key!=="trash")throw Error("Custom folder stack placement lost");
 const fileAliases=DockModel.build(reference,entries,[],[],[],false,[],["/fixture/Literal # report.txt"]);
 if(fileAliases[1].action!=="file" || fileAliases[1].path!=="/fixture/Literal # report.txt" || fileAliases[fileAliases.length-1].key!=="trash")throw Error("File alias identity lost");
 if(DockModel.recentApplications([entries[0],entries[2]],entries[0],2).length!==2)throw Error("Recent dedup lost entry");
 console.log("PASS dynamic Dock identity, deduplication, working pins and terminal tail");
 } catch(e){console.error("FAIL "+e)}Qt.quit();}
}''')
            runtime=root/'runtime';runtime.mkdir(mode=0o700)
            home=root/'home';home.mkdir()
            env=dict(os.environ,HOME=str(home),XDG_RUNTIME_DIR=str(runtime),QT_QPA_PLATFORM='offscreen',QT_QPA_PLATFORMTHEME='',QT_QUICK_BACKEND='software',XDG_DATA_HOME=str(root/'data'),XDG_CONFIG_HOME=str(root/'config'),XDG_CACHE_HOME=str(root/'cache'))
            env.update(QT_ACCESSIBILITY='0',QT_LINUX_ACCESSIBILITY_ALWAYS_ON='0',NO_AT_BRIDGE='1',GTK_A11Y='none')
            env.pop('AT_SPI_BUS_ADDRESS',None)
            env.pop('WAYLAND_DISPLAY',None)
            try:
                run=subprocess.run(['quickshell','-p',str(root)],env=env,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,timeout=10)
            except subprocess.TimeoutExpired as exc:
                self.fail('Quickshell fixture timed out: '+repr(exc.stdout))
            self.assertEqual(run.returncode,0,run.stdout)
            self.assertIn('PASS dynamic Dock',run.stdout)
            self.assertNotIn('FAIL ',run.stdout)

if __name__=='__main__':unittest.main()

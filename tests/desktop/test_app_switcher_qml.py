"""App snapshot stability, cancellation and replacement safety in Qt's JS engine."""
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

ROOT=Path(__file__).resolve().parents[2]

class AppSwitcherTests(unittest.TestCase):
    def test_group_snapshot_and_selection(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary)
            shutil.copy(ROOT/'res/lumen-desktop/shell/AppSwitcherModel.js',root/'AppSwitcherModel.js')
            (root/'shell.qml').write_text('''import QtQuick
import Quickshell
import "AppSwitcherModel.js" as Model
Timer {interval:50;running:true
 onTriggered:{try {
 const a1={app:"a"},a2={app:"a"},b1={app:"b"},c1={app:"c"},gone={app:"gone"};
 const identity=w=>w.app;
 const items=Model.snapshot([a1,a2,b1,c1],[a2,b1,gone,a1],identity);
 if(JSON.stringify(items.map(i=>i.key))!==JSON.stringify(["a","b","c"]))throw Error("App grouping or MRU lost");
 if(items[0].window!==a2)throw Error("Not most recent app window");
 if(Model.step(0,3,1)!==1 || Model.step(0,3,-1)!==2 || Model.step(2,3,1)!==0)throw Error("Cycle bounds");
 if(Model.step(0,0,1)!==-1)throw Error("Empty cycle");
 if(Model.validSelection(items,1,[a1,c1],identity)!==null)throw Error("Vanished application reused");
 const b2={app:"b"};
 if(Model.validSelection(items,1,[a1,b2],identity)!==items[1])throw Error("Same app replacement lost");
 if(Model.validSelection(items,9,[a1,b1],identity)!==null)throw Error("Invalid index accepted");
 if(JSON.stringify(items.map(i=>i.key))!==JSON.stringify(["a","b","c"]))throw Error("Snapshot mutated");
 const creation=Model.creationOrder([a1,b1],[b1,a1,a2]);
 if(creation[0]!==a1 || creation[1]!==b1 || creation[2]!==a2)throw Error("Creation order follows tree reorder");
 if(Model.cycleWindow(creation,[a1,a2,b1],a1,identity,1)!==a2)throw Error("Window cycle crossed app");
 if(Model.cycleWindow(creation,[a1,a2,b1],a1,identity,-1)!==a2)throw Error("Reverse window cycle failed");
 if(Model.cycleWindow(creation,[a1,a2,b1],b1,identity,1)!==b1)throw Error("Single window cycled another app");
 console.log("PASS app switcher grouping immutable MRU and stale selection");
 }catch(e){console.error("FAIL "+e)}Qt.quit();}
}''')
            runtime=root/'runtime';runtime.mkdir(mode=0o700)
            home=root/'home';home.mkdir()
            env=dict(os.environ,HOME=str(home),XDG_RUNTIME_DIR=str(runtime),QT_QPA_PLATFORM='offscreen',QT_QPA_PLATFORMTHEME='',QT_QUICK_BACKEND='software',XDG_DATA_HOME=str(root/'data'),XDG_CONFIG_HOME=str(root/'config'),XDG_CACHE_HOME=str(root/'cache'))
            env.pop('WAYLAND_DISPLAY',None)
            try:
                result=subprocess.run(['quickshell','-p',str(root)],env=env,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,timeout=10)
            except subprocess.TimeoutExpired as exc:
                self.fail('Quickshell fixture timed out: '+repr(exc.stdout))
            self.assertEqual(result.returncode,0,result.stdout)
            self.assertIn('PASS app switcher',result.stdout)
            self.assertNotIn('FAIL ',result.stdout)

if __name__=='__main__':unittest.main()

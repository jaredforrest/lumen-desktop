"""Exercise input ownership and application activation in the actual JS engine."""
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

ROOT=Path(__file__).resolve().parents[2]

class DockInteractionTests(unittest.TestCase):
    def test_press_identity_and_app_activation(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary)
            shutil.copy(ROOT/'res/lumen-desktop/shell/DockInteraction.js',root/'DockInteraction.js')
            (root/'shell.qml').write_text('''import QtQuick
import Quickshell
import "DockInteraction.js" as Interaction
Timer {interval:50;running:true
 onTriggered:{try {
 const a={key:"a"},b={key:"b"},replacement={key:"c"};
 function equal(actual,expected){if(actual!==expected)throw Error("Wrong target");}
 equal(Interaction.releaseTarget([a,b],"a",0),a);
 equal(Interaction.releaseTarget([a,b],"a",1),null);
 equal(Interaction.releaseTarget([replacement,b],"a",0),null);
 equal(Interaction.releaseTarget([b,a],"a",0),null);
 equal(Interaction.releaseTarget([a,b],"a",-1),null);
 equal(Interaction.releaseTarget([a,b],"",0),null);
 const a1={},a2={},b1={},destroyed={};
 equal(Interaction.activationWindow([a1,a2],a1,[a2,a1]),a1);
 equal(Interaction.activationWindow([a1,a2],b1,[destroyed,a2,a1]),a2);
 equal(Interaction.activationWindow([],b1,[a1]),null);
 equal(Interaction.activationWindow([a1],b1,[]),a1);
 console.log("PASS Dock press ownership and app MRU activation");
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
            self.assertIn('PASS Dock press',result.stdout)
            self.assertNotIn('FAIL ',result.stdout)

if __name__=='__main__':unittest.main()

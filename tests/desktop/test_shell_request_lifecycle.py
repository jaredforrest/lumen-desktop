"""Execute real shell component request boundaries in an isolated Qt process.

No compositor, account data, audio hardware or notification daemon is touched.
"""
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile
import unittest


class RequestLifecycleTests(unittest.TestCase):
    def test_stale_search_preview_and_device_selection(self):
        root = Path(__file__).resolve().parents[2]
        launcher = os.environ.get('LUMEN_TEST_SHELL_LAUNCHER') or shutil.which('lumen-desktop-shell')
        assets = Path(os.environ.get('LUMEN_TEST_SHELL_ASSETS', root/'res/lumen-desktop/shell'))
        self.assertIsNotNone(launcher, 'Installed Qt runtime is required')
        prefix = re.split(r'\n[ \t]*exec quickshell ', Path(launcher).read_text(), maxsplit=1)[0]
        raw = subprocess.check_output(['bash', '-c', prefix+'\nexec env -0'])
        installed = dict(v.decode().split('=', 1) for v in raw.split(b'\0') if b'=' in v)
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            fake = directory/'search-fixture.py'
            fake.write_text('import sys\nfor line in sys.stdin: pass\n')
            qml = directory/'shell.qml'
            qml.write_text('''import QtQuick
import Quickshell
import "file:'''+str(assets)+'''" as D
ShellRoot {
 D.SettingsState {id:settings;active:false}
 D.ClipboardState {id:clipboard;executable:"'''+str(shutil.which('true'))+'''"}
 D.ControlTile {id:focus;toggle:true;active:true;label:"Focus";subtitle:"Do Not Disturb";unavailable:false}
 QtObject {id:notifications;property bool dnd:false;property bool available:true;property var expandedCards:({});function command(data){} function invoke(row,key){} function toggleExpanded(id){}}
 QtObject {id:audioState;property bool muted:false;property bool available:false}
 QtObject {id:exported;property var state:({menu:null})}
 QtObject {id:stub;property var notifications:notifications;property var audioState:audioState;property var exportedMenu:exported;property var entries:[];property var search:search;property date currentDate:new Date();function appLabel(e){return e.name;}function appIcon(e){return "";}function preferredOutput(){return "fixture";}}
 Item {width:1000;height:900;D.AppLauncher {id:launcher;visible:false;shell:stub}}
 Item {width:344;D.NotificationCard {id:card;width:344;service:notifications;notification:({id:1,app:"Fixture",summary:"First",body:"",timestamp:Date.now()-600000,actions:[],desktop:"",icon:"",image:""})}}
 D.SearchState {id:search;helper:"'''+str(fake)+'''"}
 D.BrightnessControl {id:brightness;visible:false}
 Item {width:300;D.AudioDeviceControl {id:audio;visible:false;node:null}}
 QtObject {id:first;property int id:10;property string name:"First";property QtObject audio:QtObject {property real volume:0.4;property bool muted:false}}
 QtObject {id:second;property int id:20;property string name:"Second";property QtObject audio:QtObject {property real volume:0.7;property bool muted:false}}
 function check(value,message){if(!value)throw new Error(message);}
 Component.onCompleted: {
  search.active=true;search.search("first");search.searchId=10;
  search.requestPreview("/fixture/first");search.previewId=11;
  search.search("second");
  search.receive({id:10,results:[{path:"wrong"}]});
  search.receive({id:11,type:"text",text:"wrong"});
  check(search.files.length===0,"Old query populated new results");
  check(Object.keys(search.preview).length===0,"Old preview survived query change");
  search.searchId=12;search.receive({id:12,results:[{path:"correct"}]});
  check(search.files[0].path==="correct","Current query was discarded");
  search.receive({id:12,results:[{path:"duplicate"}]});
  check(search.files[0].path==="correct","Duplicate response was accepted");
  search.previewId=13;search.searchId=14;search.active=false;
  search.receive({id:13,type:"text",text:"closed"});search.receive({id:14,results:[{path:"closed"}]});
  check(search.files.length===0 && Object.keys(search.preview).length===0,"Closed overlay accepted data");
  search.active=true;search.search("   ");check(!search.searching,"Empty query remained busy");
  brightness.displays=[{bus:"one",name:"First display",level:0.2,maximum:100},{bus:"two",name:"Second display",level:0.8,maximum:100}];
  brightness.selectedBus="one";brightness.pending=0.3;brightness.selectedBus="two";
  check(brightness.pending===-1 && brightness.level===0.8,"Pending write migrated to another display");
  audio.node=first;audio.pendingVolume=0.3;audio.pendingMute=true;audio.error="old device";audio.node=second;
  check(audio.pendingVolume===-1 && audio.pendingMute===null && audio.error==="","Pending sound state migrated devices");
  stub.entries=[{id:"alpha",name:"Alpha"},{id:"beta",name:"Beta"}];
  launcher.setProvider("Applications");launcher.selectedIndex=1;
  const selectedIdentity=launcher.selectedKey;
  stub.entries=[{id:"earlier",name:"Aardvark"},{id:"alpha",name:"Alpha"},{id:"beta",name:"Beta"}];
  check(launcher.current.id==="beta" && launcher.selectedKey===selectedIdentity,"Search refresh moved selection to another identity");
  D.Theme.searchPreferences=({disabledProviders:["applications","files","actions","clipboard"]});
  check(launcher.providerChoices.length===1 && launcher.providerChoices[0]==="All","Disabled providers remained navigable");
  launcher.setProvider("Applications");check(launcher.provider==="All","Disabled provider accepted keyboard navigation");
  check(launcher.results.length===0,"Disabled providers exposed stale results");
  D.Theme.searchPreferences=({disabledProviders:[]});launcher.setProvider("Applications");
  check(launcher.results.length===3,"Re-enabled provider did not return results");
  clipboard.listWhenActive=false;clipboard.active=true;clipboard.refreshPending=false;
  clipboard.receive(JSON.stringify({enabled:true}),"status",clipboard.revision);
  check(!clipboard.refreshPending,"Status-only settings requested clipboard text");
  clipboard.active=false;clipboard.listWhenActive=true;
  D.Theme.searchPreferences=({disabledProviders:[],queryHistory:true});
  const recent=launcher.children.find(child=>child.objectName==="search-history-state");
  check(!!recent,"Search history state was not registered");recent.queries=["older query","oldest query"];
  launcher.recallHistory(1);check(launcher.queryText==="older query","Previous query was not recalled");
  launcher.recallHistory(1);check(launcher.queryText==="oldest query","Older query was not recalled");
  launcher.recallHistory(-1);check(launcher.queryText==="older query","Forward history was not recalled");
  D.Theme.searchPreferences=({disabledProviders:[],queryHistory:false});
  check(recent.queries.length===0,"Disabled history retained visible query text");
  const oldTime=card.timeLabel;
  card.notification=({id:1,app:"Fixture",summary:"Replacement",body:"",timestamp:Date.now(),actions:[],desktop:"",icon:"",image:""});
  check(card.timeLabel==="now" && oldTime!==card.timeLabel,"Notification replacement retained stale age");
  clipboard.active=true;clipboard.enabled=true;clipboard.revision=3;
  clipboard.receive(JSON.stringify({enabled:true,entries:[{id:"old",text:"old"}]}),"list",2);
  check(clipboard.entries.length===0,"Stale clipboard query was displayed");
  clipboard.receive(JSON.stringify({enabled:true,entries:[{id:"current",text:"literal"}]}),"list",3);
  check(clipboard.entries[0].id==="current","Current clipboard query was discarded");
  clipboard.receive(JSON.stringify({enabled:true,entries:"invalid"}),"list",3);
  check(clipboard.entries.length===0 && !!clipboard.error,"Malformed clipboard rows were retained");
  clipboard.active=false;clipboard.receive(JSON.stringify({enabled:true,entries:[{id:"closed",text:"hidden"}]}),"list",4);
  check(clipboard.entries.length===0,"Dismissed clipboard overlay repopulated sensitive text");
  clipboard.receive(JSON.stringify({enabled:false}),"disable",4);
  check(!clipboard.enabled && clipboard.entries.length===0,"Disable retained clipboard history");
  clipboard.receive(JSON.stringify({copied:true}),"copy",4);
  check(clipboard.message.includes("Paste"),"Copy did not explain explicit paste");
  console.log("PASS shell request lifecycle");Qt.quit();
 }
 Timer {interval:3000;running:true;onTriggered:Qt.quit()}
}
''')
            environment = dict(os.environ)
            environment.update({k:installed[k] for k in ('PATH','QT_PLUGIN_PATH','QML_IMPORT_PATH','GLOBAL_MENU_PYTHON') if k in installed})
            environment.update(QT_QPA_PLATFORM='offscreen', QT_QPA_PLATFORMTHEME='', QT_QUICK_BACKEND='software', HOME=str(directory), XDG_RUNTIME_DIR=str(directory), XDG_STATE_HOME=str(directory/'state'), XDG_CONFIG_HOME=str(directory/'config'), XDG_DATA_HOME=str(directory/'data'), XDG_CACHE_HOME=str(directory/'cache'))
            environment.update(QT_ACCESSIBILITY='0',QT_LINUX_ACCESSIBILITY_ALWAYS_ON='0',NO_AT_BRIDGE='1',GTK_A11Y='none')
            environment.pop('AT_SPI_BUS_ADDRESS',None)
            for key in ('DISPLAY','WAYLAND_DISPLAY','SWAYSOCK'):
                environment.pop(key, None)
            result = subprocess.run(['dbus-run-session','--','quickshell','-p',str(qml)], env=environment, capture_output=True, text=True, timeout=12)
            output = result.stdout+result.stderr
            self.assertEqual(result.returncode, 0, output)
            self.assertIn('PASS shell request lifecycle', output)
            self.assertNotIn('Error:', output)


if __name__ == '__main__':
    unittest.main()

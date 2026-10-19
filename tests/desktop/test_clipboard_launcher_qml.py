"""Real launcher and child process protocol with a private clipboard CLI fixture."""
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile
import unittest


class ClipboardLauncherTests(unittest.TestCase):
    def test_consent_query_copy_clear_disable_flow(self):
        repo = Path(__file__).resolve().parents[2]
        assets = Path(os.environ.get('LUMEN_TEST_SHELL_ASSETS', repo/'res/lumen-desktop/shell'))
        prefix = re.split(r'\n[ \t]*exec quickshell ', Path(os.environ.get('LUMEN_TEST_SHELL_LAUNCHER') or shutil.which('lumen-desktop-shell')).read_text(), maxsplit=1)[0]
        raw = subprocess.check_output(['bash','-c',prefix+'\nexec env -0'])
        installed = dict(v.decode().split('=',1) for v in raw.split(b'\0') if b'=' in v)
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            binary = root/'bin';binary.mkdir()
            cli = binary/'lumen-clipboard-history'
            cli.write_text('#!'+sys.executable+'''\nimport json,os,sys
from pathlib import Path
root=Path(os.environ['CLIPBOARD_FIXTURE']);state=root/'fixture-state.json'
s=json.loads(state.read_text()) if state.exists() else {'enabled':False,'entries':[{'id':'fixture-one','text':'Fixture copied text','timestamp':1}]}
a=sys.argv[1]
with (root/'commands.jsonl').open('a') as f:f.write(json.dumps(sys.argv[1:])+'\\n')
if a=='enable':s['enabled']=True
elif a=='disable':s.update(enabled=False,entries=[])
elif a=='clear':s['entries']=[]
result={'enabled':s['enabled']}
if a=='list':result['entries']=[e for e in s['entries'] if sys.argv[3].casefold() in e['text'].casefold()]
if a=='copy':
 assert s['enabled'] and any(e['id']==sys.argv[2] for e in s['entries'])
 result['copied']=True
state.write_text(json.dumps(s));print(json.dumps(result))
''');cli.chmod(0o700)
            qml=root/'shell.qml'
            qml.write_text('''import QtQuick
import Quickshell
import "file:'''+str(assets)+'''" as D
ShellRoot {
 id:root
 property int phase:0
 property int ticks:0
 D.SearchState {id:search}
 QtObject {id:notifications;property bool dnd:false;property bool available:false}
 QtObject {id:audio;property bool muted:false;property bool available:false}
 QtObject {id:exported;property var state:({menu:null})}
 QtObject {id:stub;property var search:search;property var notifications:notifications;property var audioState:audio;property var exportedMenu:exported;property var entries:[];property date currentDate:new Date();function appLabel(e){return e.name;}}
 Item {width:1000;height:900;D.AppLauncher {id:launcher;shell:stub}}
 function find(item,predicate){if(predicate(item))return item;for(const child of item.children || []){const match=find(child,predicate);if(match)return match;}return null;}
 function press(text){const button=find(launcher,item=>item.text===text && typeof item.clicked==="function");if(!button)throw new Error("Button unavailable: "+text);button.clicked();}
 Timer {interval:50;repeat:true;running:true;onTriggered:{
  try {
   if(++root.ticks>120)throw new Error("Timed out phase "+root.phase);
   const state=root.find(launcher,item=>item.objectName==="clipboard-history-state");
   if(root.phase===0){launcher.setProvider("Clipboard");root.phase=1;}
   else if(root.phase===1 && state.available && !state.busy){if(state.enabled)throw new Error("Consent skipped");root.press("Enable Clipboard History");root.phase=2;}
   else if(root.phase===2 && state.enabled && !state.busy && launcher.results.length===1){state.search("copied");root.phase=3;}
   else if(root.phase===3 && !state.busy && launcher.results.length===1){launcher.launch(0,false);root.phase=4;}
   else if(root.phase===4 && !state.busy && state.message.includes("Paste")){if(!launcher.visible)throw new Error("Copy closed overlay");root.press("Clear History");root.phase=5;}
   else if(root.phase===5 && !state.busy && state.entries.length===0){root.press("Disable History");root.phase=6;}
   else if(root.phase===6 && !state.busy && !state.enabled){console.log("PASS clipboard launcher protocol");Qt.quit();}
  }catch(error){console.error("FAIL "+error);Qt.quit();}
 }}
}
''')
            env=dict(os.environ)
            env.update({k:installed[k] for k in ('PATH','QT_PLUGIN_PATH','QML_IMPORT_PATH','GLOBAL_MENU_PYTHON') if k in installed})
            env.update(PATH=str(binary)+':'+env['PATH'],QT_QPA_PLATFORM='offscreen',QT_QPA_PLATFORMTHEME='',QT_QUICK_BACKEND='software',HOME=str(root),XDG_RUNTIME_DIR=str(root),XDG_STATE_HOME=str(root/'state'),XDG_CONFIG_HOME=str(root/'config'),XDG_DATA_HOME=str(root/'data'),XDG_CACHE_HOME=str(root/'cache'),CLIPBOARD_FIXTURE=str(root))
            env.update(QT_ACCESSIBILITY='0',QT_LINUX_ACCESSIBILITY_ALWAYS_ON='0',NO_AT_BRIDGE='1',GTK_A11Y='none')
            env.pop('AT_SPI_BUS_ADDRESS',None)
            for key in ('DISPLAY','WAYLAND_DISPLAY','SWAYSOCK'):
                env.pop(key,None)
            result=subprocess.run(['dbus-run-session','--','quickshell','-p',str(qml)],env=env,text=True,capture_output=True,timeout=12)
            output=result.stdout+result.stderr
            self.assertEqual(result.returncode,0,output)
            self.assertIn('PASS clipboard launcher protocol',output)
            self.assertNotIn('FAIL ',output)
            self.assertNotIn('Error:',output)
            commands=[json.loads(line) for line in (root/'commands.jsonl').read_text().splitlines()]
            self.assertEqual(commands[0],['status'])
            self.assertIn(['enable'],commands)
            self.assertIn(['list','--query','copied'],commands)
            self.assertIn(['copy','fixture-one'],commands)
            self.assertIn(['clear'],commands)
            self.assertEqual(commands[-1],['disable'])


if __name__=='__main__':unittest.main()

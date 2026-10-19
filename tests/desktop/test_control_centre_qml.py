"""Real Qt control layout: hide/reorder and queued declarative preference writes."""
import os,re,shutil,subprocess,tempfile
from pathlib import Path
repo=Path(__file__).resolve().parents[2]
assets=Path(os.environ.get('LUMEN_TEST_SHELL_ASSETS',repo/'res/lumen-desktop/shell'))
launcher=os.environ.get('LUMEN_TEST_SHELL_LAUNCHER') or shutil.which('lumen-desktop-shell')
prefix=re.split(r'\n[ \t]*exec quickshell ',Path(launcher).read_text(),maxsplit=1)[0]
raw=subprocess.check_output(['bash','-c',prefix+'\nexec env -0'])
env=dict(v.decode().split('=',1) for v in raw.split(b'\0') if b'=' in v)
with tempfile.TemporaryDirectory() as directory:
 root=Path(directory)
 env.update(HOME=directory,XDG_RUNTIME_DIR=directory,XDG_STATE_HOME=directory,XDG_CONFIG_HOME=directory,QT_QPA_PLATFORM='offscreen',QT_QUICK_BACKEND='software')
 for key in ('WAYLAND_DISPLAY','DISPLAY'):env.pop(key,None)
 binary=root/'bin';binary.mkdir();helper=binary/'lumen-appearance';helper.write_text('#!/bin/sh\nprintf \'%s %s\\n\' \"$2\" \"$3\" >> \"$FIXTURE_LOG\"\nprintf \'{}\\n\'\n');helper.chmod(0o700)
 env['FIXTURE_BIN']=str(binary);env['FIXTURE_LOG']=str(root/'writes')
 qml=root/'shell.qml'
 qml.write_text('''import QtQuick
import Quickshell
import "file:'''+str(assets)+'''" as D
ShellRoot {
 property int phase:0
 property int ticks:0
 QtObject {id:notifications;property bool available:true;property bool dnd:false;property var focusState:({name:""})}
 QtObject {id:system;property var wifi:({available:false,state:""})}
 QtObject {id:audio;property bool available:false;property bool muted:false;property real volume:0;property var outputs:[]}
 QtObject {id:stub;property var notifications:notifications;property var systemState:system;property var audioState:audio}
 D.ControlCentre {id:editor;shell:stub;preferences:({order:["wifi","bluetooth","sharing","media","displays","appearance","focus","brightness","sound"],hidden:["wifi","bluetooth","sharing","media","brightness","sound"]})}
 Timer {interval:100;running:true;repeat:true;onTriggered:{
  try {
   if(++ticks>100)throw Error("Writer timeout");
   if(phase===0){editor.move("focus",-1);
   if(editor.preferences.order[5]!=="focus")throw Error("reorder failed");
   editor.toggle("displays");
   if(!editor.preferences.hidden.includes("displays"))throw Error("hide failed");
   editor.editing=true;
   if(editor.preferences.order.length!==9)throw Error("controls lost");
   phase=1;return;}
   if(editor.saving || Object.keys(editor.pending).length)return;
   console.log("FOCUS_EDITOR_PASS");Qt.quit();
  }catch(e){console.error("FOCUS_EDITOR_FAIL "+e);Qt.quit();}
 }}
}
''')
 result=subprocess.run(['dbus-run-session','--','bash','-c',prefix+'\nexport QT_QPA_PLATFORM=offscreen QT_QUICK_BACKEND=software QT_ACCESSIBILITY=0 QT_LINUX_ACCESSIBILITY_ALWAYS_ON=0 NO_AT_BRIDGE=1 GTK_A11Y=none QT_QPA_PLATFORMTHEME=generic\nexport HOME="$XDG_RUNTIME_DIR" XDG_CONFIG_HOME="$XDG_RUNTIME_DIR" XDG_CACHE_HOME="$XDG_RUNTIME_DIR" XDG_DATA_HOME="$XDG_RUNTIME_DIR"\nexport PATH="$FIXTURE_BIN:$PATH"\nunset WAYLAND_DISPLAY DISPLAY\nexec quickshell -p "$1"','fixture',str(qml)],env=env,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,timeout=30)
 if 'FOCUS_EDITOR_PASS' not in result.stdout or 'FOCUS_EDITOR_FAIL' in result.stdout or 'Failed to load' in result.stdout or 'ReferenceError:' in result.stdout or 'TypeError:' in result.stdout or 'Unable to assign' in result.stdout:raise AssertionError(result.stdout)
 writes=(root/'writes').read_text();assert 'order [' in writes and 'hidden [' in writes,writes
 print('PASS actual Qt editable Control Centre: ordered controls, hide/show, edit view and declarative writer')

"""Load the actual Focus editor with Qt and exercise its policy update helpers."""
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
 qml=root/'shell.qml'
 qml.write_text('''import QtQuick
import Quickshell
import "file:'''+str(assets)+'''" as D
ShellRoot {
 QtObject {id:service;property var focusState:({profiles:[{id:"work",name:"Work",allowedApps:[],allowCritical:false,schedules:[{days:[0],start:540,end:1020}]}]});property var applications:[{key:"chat",name:"Chat"}];property var last:null;function command(data){last=data;focusState={profiles:[data.profile]};}}
 D.FocusProfileEditor {id:editor;service:service;profileId:"work"}
 Timer {interval:100;running:true;onTriggered:{
  try {
   if(editor.profile.name!=="Work")throw Error("profile not loaded");
   editor.update("allowedApps",["chat"]);
   if(service.last.profile.allowedApps[0]!=="chat")throw Error("exception not sent");
   editor.schedule(0,"end",600);
   if(service.last.profile.schedules[0].end!==600)throw Error("schedule not sent");
   if(service.last.profile.name!=="Work" || service.last.profile.allowedApps[0]!=="chat")throw Error("sibling policy lost");
   console.log("FOCUS_EDITOR_PASS");Qt.quit();
  }catch(e){console.error("FOCUS_EDITOR_FAIL "+e);Qt.quit();}
 }}
}
''')
 result=subprocess.run(['dbus-run-session','--','bash','-c',prefix+'\nexport QT_QPA_PLATFORM=offscreen QT_QUICK_BACKEND=software QT_ACCESSIBILITY=0 QT_LINUX_ACCESSIBILITY_ALWAYS_ON=0 NO_AT_BRIDGE=1 GTK_A11Y=none QT_QPA_PLATFORMTHEME=generic\nexport HOME="$XDG_RUNTIME_DIR" XDG_CONFIG_HOME="$XDG_RUNTIME_DIR" XDG_CACHE_HOME="$XDG_RUNTIME_DIR" XDG_DATA_HOME="$XDG_RUNTIME_DIR"\nunset WAYLAND_DISPLAY DISPLAY\nexec quickshell -p "$1"','fixture',str(qml)],env=env,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,timeout=30)
 if 'FOCUS_EDITOR_PASS' not in result.stdout or 'FOCUS_EDITOR_FAIL' in result.stdout or 'Failed to load' in result.stdout:raise AssertionError(result.stdout)
 print('PASS actual Qt Focus editor instantiation, exception/schedule commands and sibling preservation')

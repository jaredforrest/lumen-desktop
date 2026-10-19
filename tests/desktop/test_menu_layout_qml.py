"""Actual QML menu partition logic at collision and overflow boundaries."""
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
import "file:ASSETS/MenuLayout.js" as L
ShellRoot {
 Timer {interval:50;running:true;onTriggered:{
  try {
   if(L.fit([40,50],90,30,false)!==2)throw Error("exact fit");
   if(L.fit([40,50],89,30,false)!==1)throw Error("overflow reservation");
   if(L.fit([40,50],90,30,true)!==1)throw Error("status overflow reservation");
   if(L.fit([40],10,30,true)!==0)throw Error("small width");
   if(L.fit([],0,30,false)!==0)throw Error("empty model");
   console.log("FOCUS_EDITOR_PASS");Qt.quit();
  }catch(e){console.error("FOCUS_EDITOR_FAIL "+e);Qt.quit();}
 }}
}
'''.replace('ASSETS',str(assets)))
 result=subprocess.run(['dbus-run-session','--','bash','-c',prefix+'\nexport QT_QPA_PLATFORM=offscreen QT_QUICK_BACKEND=software QT_ACCESSIBILITY=0 QT_LINUX_ACCESSIBILITY_ALWAYS_ON=0 NO_AT_BRIDGE=1 GTK_A11Y=none QT_QPA_PLATFORMTHEME=generic\nexport HOME="$XDG_RUNTIME_DIR" XDG_CONFIG_HOME="$XDG_RUNTIME_DIR" XDG_CACHE_HOME="$XDG_RUNTIME_DIR" XDG_DATA_HOME="$XDG_RUNTIME_DIR"\nunset WAYLAND_DISPLAY DISPLAY\nexec quickshell -p "$1"','fixture',str(qml)],env=env,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,timeout=30)
 if 'FOCUS_EDITOR_PASS' not in result.stdout or 'FOCUS_EDITOR_FAIL' in result.stdout or 'Failed to load' in result.stdout:raise AssertionError(result.stdout)
 print('PASS menu heading partition exact-fit, overflow reservation, status-only overflow and minimum bounds')

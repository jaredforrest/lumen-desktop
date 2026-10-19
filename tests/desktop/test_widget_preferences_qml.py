"""Real Qt widget policy migration, validation and persistence without a live desktop."""
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
 (root/'lumen-desktop/session').mkdir(parents=True)
 qml=root/'shell.qml'
 qml.write_text('''import QtQuick
import Quickshell
import "file:'''+str(assets)+'''" as D
ShellRoot {
 D.WidgetState {id:state}
 Timer {interval:200;running:true;onTriggered:{
  try {
   const clamped=state.validate({desktopVisible:true,desktopColor:"bad",positions:{screen:{x:-2,y:5}}});
   if(clamped.positions.screen.x!==0 || clamped.positions.screen.y!==1 || clamped.desktopColor!=="auto")throw Error("invalid geometry accepted");
   state.setPreference("desktopVisible",true);state.setPreference("desktopStyle","agenda");state.setPreference("desktopColor","monochrome");state.setPosition("test-output",0.25,0.5);
   if(!state.preferences.calendarVisible || !state.preferences.desktopVisible || state.preferences.desktopStyle!=="agenda")throw Error("widget preferences lost");
   console.log("FOCUS_EDITOR_PASS");
  }catch(e){console.error("FOCUS_EDITOR_FAIL "+e);}
 }}
 Timer {interval:600;running:true;onTriggered:Qt.quit()}
}
'''.replace('ASSETS',str(assets)))
 result=subprocess.run(['dbus-run-session','--','bash','-c',prefix+'\nexport QT_QPA_PLATFORM=offscreen QT_QUICK_BACKEND=software QT_ACCESSIBILITY=0 QT_LINUX_ACCESSIBILITY_ALWAYS_ON=0 NO_AT_BRIDGE=1 GTK_A11Y=none QT_QPA_PLATFORMTHEME=generic\nexport HOME="$XDG_RUNTIME_DIR" XDG_CONFIG_HOME="$XDG_RUNTIME_DIR" XDG_CACHE_HOME="$XDG_RUNTIME_DIR" XDG_DATA_HOME="$XDG_RUNTIME_DIR"\nunset WAYLAND_DISPLAY DISPLAY\nexec quickshell -p "$1"','fixture',str(qml)],env=env,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,timeout=30)
 if 'FOCUS_EDITOR_PASS' not in result.stdout or 'FOCUS_EDITOR_FAIL' in result.stdout or 'Failed to load' in result.stdout or 'ReferenceError:' in result.stdout or 'TypeError:' in result.stdout:raise AssertionError(result.stdout)
 data=__import__('json').loads((root/'lumen-desktop/session/notification-centre.json').read_text());assert data['desktopVisible'] and data['positions']['test-output']=={'x':0.25,'y':0.5}
 print('PASS Qt desktop widget configuration, position clamps, independent layouts, persistence')

"""Missing optional consumer environment cannot start a sharing provider."""
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile

repo=Path(__file__).resolve().parents[2]
launcher=Path(os.environ.get('LUMEN_TEST_SHELL_LAUNCHER') or shutil.which('lumen-desktop-shell'))
prefix=re.split(r'\n[ \t]*exec quickshell ',launcher.read_text(),maxsplit=1)[0]
with tempfile.TemporaryDirectory(prefix='lumen-sharing-default-') as directory:
 root=Path(directory);binary=root/'bin';binary.mkdir();helper=binary/'lumen-nearby-sharing'
 helper.write_text('#!/bin/sh\nprintf unexpected > "$HOME/provider-executed"\n');helper.chmod(0o700)
 qml=root/'shell.qml';qml.write_text('''import QtQuick
import Quickshell
import "file:'''+str(repo/'res/lumen-desktop/shell')+'''" as D
ShellRoot {
 Component.onCompleted:D.NearbySharingState.refresh()
 Timer {interval:300;running:true;onTriggered:{
  if(D.NearbySharingState.label!=="Nearby Sharing" || D.NearbySharingState.configured || D.NearbySharingState.available || D.NearbySharingState.active || D.NearbySharingState.canSend)console.error("SHARING_DEFAULT_FAIL");
  else console.log("SHARING_DEFAULT_PASS");
  Qt.quit();
 }}
}
''')
 env=dict(os.environ,HOME=directory,XDG_RUNTIME_DIR=directory,XDG_CONFIG_HOME=directory,XDG_STATE_HOME=directory,XDG_CACHE_HOME=directory,XDG_DATA_HOME=directory,DBUS_SYSTEM_BUS_ADDRESS='unix:path='+directory+'/absent-system',PULSE_SERVER='unix:'+directory+'/absent-audio',FIXTURE_BIN=str(binary))
 command=prefix+'''\nexport QT_QPA_PLATFORM=offscreen QT_QUICK_BACKEND=software QT_QPA_PLATFORMTHEME=generic QT_ACCESSIBILITY=0 QT_LINUX_ACCESSIBILITY_ALWAYS_ON=0 NO_AT_BRIDGE=1 GTK_A11Y=none
export PATH="$FIXTURE_BIN:$PATH"
unset LUMEN_NEARBY_SHARING_PROVIDER LUMEN_NEARBY_SHARING_LABEL WAYLAND_DISPLAY DISPLAY
exec quickshell -p "$1"'''
 result=subprocess.run(['dbus-run-session','--','bash','-c',command,'fixture',str(qml)],env=env,text=True,capture_output=True,timeout=15)
 output=result.stdout+result.stderr
 assert result.returncode==0 and 'SHARING_DEFAULT_PASS' in output and not any(text in output for text in ('SHARING_DEFAULT_FAIL','TypeError:','ReferenceError:','Unable to assign','Failed to load')),output
 assert not (root/'provider-executed').exists()
 print('PASS actual Qt missing consumer environment: default label, unavailable capability and no provider execution')

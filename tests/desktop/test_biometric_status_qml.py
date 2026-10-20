"""Actual inactive settings pane: readable discovery states, no authentication action."""
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile

repo = Path(__file__).resolve().parents[2]
launcher = os.environ.get('LUMEN_TEST_SHELL_LAUNCHER') or shutil.which('lumen-desktop-shell')
prefix = re.split(r'\n[ \t]*exec quickshell ', Path(launcher).read_text(), maxsplit=1)[0]
with tempfile.TemporaryDirectory(prefix='lumen-biometric-pane-') as directory:
    root = Path(directory)
    env = dict(os.environ, HOME=directory, XDG_RUNTIME_DIR=directory,
               XDG_CONFIG_HOME=directory, XDG_DATA_HOME=directory,
               XDG_STATE_HOME=directory, XDG_CACHE_HOME=directory,
               DBUS_SYSTEM_BUS_ADDRESS='unix:path=' + directory + '/absent-system-bus')
    for key in ('SWAYSOCK', 'WAYLAND_DISPLAY', 'DISPLAY', 'AT_SPI_BUS_ADDRESS', 'DBUS_SESSION_BUS_ADDRESS'):
        env.pop(key, None)
    qml = root / 'shell.qml'
    qml.write_text('''import QtQuick
import Quickshell
import "file:''' + str(repo / 'res/lumen-desktop/shell') + '''" as D
ShellRoot {
 property int step:0
 property var cases:[['unknown','Provider inactive'],['unavailable','No reader reported'],['reported','Reader reported'],['unknown','Unavailable']]
 D.PrivacySecurityPane {id:pane;width:360;active:false}
 function find(item){if(item.Accessible && (item.Accessible.name || '').startsWith('Fingerprint reader.'))return item;for(const child of item.children || []){const result=find(child);if(result)return result;}return null;}
 Timer {interval:80;running:true;repeat:true;onTriggered:{try{
  if(step>0){const row=find(pane);if(!row || row.Accessible.role!==Accessible.StaticText || !row.Accessible.name.includes(cases[step-1][1]) || !row.Accessible.name.includes('Not an authentication result'))throw Error('Unreadable or incorrect status');if(row.width!==360 || row.height<=0)throw Error('Invalid status layout');}
  if(step===cases.length){console.log('BIOMETRIC_PANE_PASS');Qt.quit();return;}
  const value=cases[step++];pane.facts={biometrics:{state:value[0],value:value[1],detail:'Not an authentication result'}};
 }catch(error){console.error('BIOMETRIC_PANE_FAIL '+error);Qt.quit();}}}
}
''')
    result = subprocess.run(['dbus-run-session', '--', 'bash', '-c', prefix + '\nexport QT_QPA_PLATFORM=offscreen QT_QUICK_BACKEND=software QT_ACCESSIBILITY=0 QT_LINUX_ACCESSIBILITY_ALWAYS_ON=0 NO_AT_BRIDGE=1 GTK_A11Y=none QT_QPA_PLATFORMTHEME=generic\nunset WAYLAND_DISPLAY DISPLAY\nexec quickshell -p "$1"', 'fixture', str(qml)], env=env, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, timeout=20)
    assert result.returncode == 0 and 'BIOMETRIC_PANE_PASS' in result.stdout, result.stdout
    assert not any(word in result.stdout for word in ('BIOMETRIC_PANE_FAIL', 'ReferenceError:', 'TypeError:', 'Binding loop', 'Failed to load', 'Unable to assign')), result.stdout
    print('PASS actual settings discovery states and accessible labels; no provider activation or authentication')

"""Actual editor configuration roundtrip on a private Qt runtime."""
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile
repo = Path(__file__).resolve().parents[2]
launcher = os.environ.get('LUMEN_TEST_SHELL_LAUNCHER') or shutil.which('lumen-desktop-shell')
prefix = re.split(r'\n[ \t]*exec quickshell ', Path(launcher).read_text(), maxsplit=1)[0]
raw = subprocess.check_output(['bash', '-c', prefix + '\nexec env -0'])
env = dict(v.decode().split('=', 1) for v in raw.split(b'\0') if b'=' in v)
with tempfile.TemporaryDirectory(prefix='lumen-corners-') as directory:
    root = Path(directory)
    env.update(HOME=directory, XDG_RUNTIME_DIR=directory, XDG_STATE_HOME=directory, XDG_CONFIG_HOME=directory, XDG_CACHE_HOME=directory, XDG_DATA_HOME=directory, QT_QPA_PLATFORM='offscreen', QT_QPA_PLATFORMTHEME='generic', QT_QUICK_BACKEND='software', QT_ACCESSIBILITY='0', QT_LINUX_ACCESSIBILITY_ALWAYS_ON='0', NO_AT_BRIDGE='1', GTK_A11Y='none')
    for key in ('WAYLAND_DISPLAY', 'DISPLAY', 'AT_SPI_BUS_ADDRESS', 'DBUS_SESSION_BUS_ADDRESS'):
        env.pop(key, None)
    binary = root / 'bin'
    binary.mkdir()
    command = binary / 'lumen-hot-corners'
    command.write_text('#!' + sys.executable + '\nimport json,sys\nfrom pathlib import Path\np=Path("' + str(root / 'saved.json') + '")\nif sys.argv[1]=="configure":\n p.write_text(sys.stdin.readline());print(json.dumps({"ok":True}))\nelse:\n data=json.loads(p.read_text()) if p.exists() else {c:{"action":"disabled","modifiers":[],"allowFullscreen":False,"allowAdjacent":False} for c in ["top-left","top-right","bottom-left","bottom-right"]}\n print(json.dumps({"corners":data,"nativeAvailable":True}))\n')
    command.chmod(0o700)
    env['PATH'] = str(binary) + ':' + env['PATH']
    qml = root / 'shell.qml'
    qml.write_text('''import QtQuick
import Quickshell
import "'''+(repo/'res/lumen-desktop/shell').as_uri()+'''" as D
ShellRoot {
 D.HotCornerSettings {id:editor;width:600}
 Timer {interval:400;running:true;onTriggered:{
  if(editor.busy || !editor.nativeAvailable || editor.selected.action!=="disabled")throw Error("initial state missing");
  editor.update("action","overview");editor.update("modifiers",["super","shift"]);editor.update("allowFullscreen",true);editor.save();
 }}
 Timer {interval:1000;running:true;onTriggered:{
  if(editor.busy || editor.error || editor.selected.action!=="overview" || editor.selected.modifiers.length!==2 || !editor.selected.allowFullscreen)throw Error("editor roundtrip failed");
  console.log("HOT_CORNERS_PASS");Qt.quit();
 }}
}
''')
    result = subprocess.run(['dbus-run-session', '--', 'quickshell', '-p', str(qml)], env=env, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, timeout=20)
    assert 'HOT_CORNERS_PASS' in result.stdout and 'ReferenceError' not in result.stdout and 'TypeError' not in result.stdout and 'Failed to load' not in result.stdout, result.stdout
    saved = json.loads((root / 'saved.json').read_text())
    assert saved['top-left']['action'] == 'overview' and saved['top-left']['modifiers'] == ['super', 'shift']
    print('PASS actual corner editor default, modifiers, safeguards and save/reload')

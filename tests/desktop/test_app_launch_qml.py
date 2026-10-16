"""Exercise the real Quickshell desktop-action parser without launching apps."""
import os
import shutil
import subprocess
import tempfile
from pathlib import Path

source = Path(__file__).resolve().parents[2] / 'res/lumen-desktop/shell/AppLaunch.qml'
with tempfile.TemporaryDirectory(prefix='lumen-launch-qml-') as temporary:
    root = Path(temporary)
    shutil.copy(source, root / source.name)
    (root / 'qmldir').write_text('singleton AppLaunch 1.0 AppLaunch.qml\n')
    (root / 'shell.qml').write_text('''import QtQuick
import Quickshell
import "."
Timer {
    interval: 300; running: true; repeat: false
    property var available: DesktopEntries.applications.values
    function equal(actual, expected) {
        if (JSON.stringify(actual) !== JSON.stringify(expected))
            throw new Error(JSON.stringify(actual) + " != " + JSON.stringify(expected));
    }
    onTriggered: {
        try {
            const entry = {command:["app", "$HOME", "literal value"], actions:[
                {id:"new-private-window",command:["private"]},
                {id:"new-window",command:["env", "SPECIAL=1", "app", "--new-instance"]}],
                runInTerminal:false};
            equal(AppLaunch.commandFor(entry,false), entry.command);
            equal(AppLaunch.commandFor(entry,true), entry.actions[1].command);
            entry.runInTerminal=true;
            equal(AppLaunch.commandFor(entry,true), ["foot","-e"].concat(entry.actions[1].command));
            entry.actions=[];
            equal(AppLaunch.commandFor(entry,true), ["foot","-e"].concat(entry.command));
            entry.runInTerminal=false;
            entry.actions=[{id:"NewDocument",command:["writer","--new"]}];
            equal(AppLaunch.commandFor(entry,true), ["writer","--new"]);
            // A fixture .desktop file is parsed by the installed Quickshell,
            // proving that its QList<DesktopAction*> supports this path.
            const real = DesktopEntries.byId("lumen-launch-test");
            if (!real) throw new Error("fixture desktop entry missing");
            equal(AppLaunch.commandFor(real,true), ["env","MULTI=1","app","--new-instance"]);
            console.log("PASS launch actions, literal arguments, terminal wrapper, fallback and Quickshell desktop parser");
        } catch (error) { console.error("FAIL " + error); }
        Qt.quit();
    }
}
''')
    apps = root / 'data/applications'
    apps.mkdir(parents=True)
    (apps / 'lumen-launch-test.desktop').write_text('''[Desktop Entry]
Type=Application
Name=Launch test
Exec=app
Actions=new-window;
[Desktop Action new-window]
Name=New Window
Exec=env MULTI=1 app --new-instance
''')
    env = dict(os.environ, QT_QPA_PLATFORM='offscreen', XDG_DATA_HOME=str(root / 'data'),
               XDG_CONFIG_HOME=str(root / 'config'), XDG_CACHE_HOME=str(root / 'cache'))
    env.pop('WAYLAND_DISPLAY', None)
    result = subprocess.run(['quickshell', '-p', str(root)], env=env, text=True,
                            stdout=subprocess.PIPE, stderr=subprocess.STDOUT, timeout=10)
    output = result.stdout
    print(output)
    assert result.returncode == 0 and 'PASS launch actions' in output and 'FAIL ' not in output

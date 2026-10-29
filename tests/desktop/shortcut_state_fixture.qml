import QtQuick
import Quickshell
import "LUMEN_ASSETS" as D

ShellRoot {
    D.ShortcutState { id: shortcuts }
    property string last: "unreported"
    Timer {
        running: true
        repeat: true
        interval: 25
        onTriggered: {
            const value = JSON.stringify({available: shortcuts.available,
                hint: shortcuts.hint("window.fullscreen.toggle"), bindings: shortcuts.bindings.length});
            if (value !== last) { last = value; console.log("SHORTCUT_STATE " + value); }
        }
    }
}

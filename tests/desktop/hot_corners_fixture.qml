import QtQuick
import Quickshell
import "LUMEN_ASSETS" as D

ShellRoot {
    D.HotCornerSettings { id: editor; width: 600 }
    Timer {
        interval: 400
        running: true
        onTriggered: {
            if (editor.busy || !editor.nativeAvailable || editor.selected.action !== "disabled") throw Error("Initial state missing");
            editor.update("action", "overview");
            editor.update("modifiers", ["super", "shift"]);
            editor.update("allowFullscreen", true);
            editor.save();
        }
    }
    Timer {
        interval: 1000
        running: true
        onTriggered: {
            if (editor.busy || editor.error || editor.selected.action !== "overview" || editor.selected.modifiers.length !== 2 || !editor.selected.allowFullscreen) throw Error("Editor roundtrip failed");
            console.log("HOT_CORNERS_PASS"); Qt.quit();
        }
    }
}

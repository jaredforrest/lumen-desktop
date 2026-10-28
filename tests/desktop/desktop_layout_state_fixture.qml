import QtQuick
import Quickshell

ShellRoot {
    property int phase: 0
    property int ticks: 0
    Loader {
        id: state
        Component.onCompleted: setSource("LUMEN_ASSETS/DesktopLayoutState.qml", {directory: "LUMEN_DESKTOP_FOLDER", rows: [{name: "one.txt", identity: {device: 1, inode: 2}}], active: true})
    }
    function check(value, message) { if (!value) throw Error(message); }
    Timer {
        interval: 70
        running: true
        repeat: true
        onTriggered: {
            try {
                if (++ticks > 120) throw Error("Desktop arrangement fixture timeout " + phase);
                if (!state.item || !state.item.ready || state.item.busy) return;
                const value = state.item;
                if (phase === 0) { value.move("1:2", 196, 120, 500, 400); phase = 1; return; }
                if (phase === 1) { check(value.layout.positions["1:2"][0] === .5, "Drag normalization lost"); value.rows = [{name: "renamed.txt", identity: {device: 1, inode: 2}}]; phase = 2; return; }
                if (phase === 2) { check(value.layout.positions["1:2"][1] === .4, "Rename discarded stable placement"); value.group("kind"); phase = 3; return; }
                if (phase === 3) { state.active = false; phase = 4; return; }
                if (phase === 4) return;
                if (phase === 5) { check(value.layout.grouping === "kind" && value.layout.positions["1:2"][0] === .5, "Restart did not restore metadata"); console.log("DESKTOP_LAYOUT_QT_PASS"); Qt.quit(); }
            } catch (error) { console.error("DESKTOP_LAYOUT_QT_FAIL " + error); Qt.quit(); }
        }
    }
    Timer {
        interval: 80
        running: true
        repeat: true
        onTriggered: if (phase === 4) {
            state.active = true;
            state.setSource("LUMEN_ASSETS/DesktopLayoutState.qml", {directory: "LUMEN_DESKTOP_FOLDER", rows: [{name: "renamed.txt", identity: {device: 1, inode: 2}}], active: true});
            phase = 5;
        }
    }
}

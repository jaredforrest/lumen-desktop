import QtQuick
import Quickshell
import Quickshell.Io
import "LUMEN_ASSETS" as D

ShellRoot {
    property int serial: 0
    property int phase: 0
    property double retiredAt: 0
    FileView { path: "LUMEN_CAPTURE_COUNTER"; watchChanges: true; onFileChanged: reload(); onLoaded: serial = JSON.parse(text()).serial }
    D.CaptureState { id: state; active: false }
    Timer {
        interval: 10
        running: true
        repeat: true
        triggeredOnStart: true
        onTriggered: {
            try {
                if (phase === 0) { phase = 1; state.active = true; return; }
                if (phase === 1 && serial === 1) { state.active = false; state.active = true; phase = 2; return; }
                if (phase === 2) {
                    if (state.status.available && state.status.rows[0].reportedApplication === "retired") throw Error("Retired stdout was republished");
                    if (serial === 2 && state.status.available) {
                        if (state.status.rows[0].reportedApplication !== "fresh") throw Error("Wrong generation");
                        state.active = false; retiredAt = Date.now(); phase = 3;
                    }
                    return;
                }
                if (phase === 3 && Date.now() - retiredAt > 150) {
                    if (state.audio || state.status.available) throw Error("Inactive state restored");
                    console.log("CAPTURE_RETIRE_PASS"); Qt.quit();
                }
            } catch (error) { console.error("CAPTURE_RETIRE_FAIL " + error); Qt.quit(); }
        }
    }
}

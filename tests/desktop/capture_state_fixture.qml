import QtQuick
import Quickshell
import "LUMEN_ASSETS" as D

ShellRoot {
    D.CaptureState { id: state; active: false }
    FloatingWindow {
        visible: true
        implicitWidth: 400
        implicitHeight: 600
        D.CaptureReport { id: report; width: 360; state: state }
    }
    Timer {
        interval: 100
        running: true
        onTriggered: {
            try {
                if (state.status.available || state.audio || state.video) throw Error("Initial state");
                const row = {kind: "audio-input", reportedApplication: "Synthetic recorder <literal>", attribution: "Reported metadata, not authenticated"};
                if (!state.publish({available: true, rows: [row], coverage: "Other paths not assessed"}) || !state.audio || state.video) throw Error("Audio graph");
                state.expire(state.lastPublished + 7501);
                if (state.audio || state.status.available) throw Error("Stale state survived");
                state.publish({available: true, rows: [row], coverage: "Other paths not assessed"});
                const published = state.lastPublished;
                state.expire(published + 5000);
                state.expire(published + 4000);
                if (state.audio) throw Error("Clock reversal");
                if (state.publish({available: true, rows: [{kind: "camera", reportedApplication: "invented", attribution: "wrong"}], coverage: ""}) || state.status.available) throw Error("Unknown classification accepted");
                state.publish({available: true, rows: [row, {kind: "video-input", reportedApplication: "Synthetic video", attribution: "Reported metadata"}], coverage: "Other paths not assessed"});
                if (!state.audio || !state.video || report.height <= 0 || report.height > 600) throw Error("Actual report layout");
                console.log("PASS capture state expiry");
            } catch (error) {
                console.error("FAIL " + error);
            }
            Qt.quit();
        }
    }
}

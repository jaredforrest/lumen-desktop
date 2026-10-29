import QtQuick
import Quickshell
import "assets" as D

ShellRoot {
    id: test
    property int phase: 0
    property int ticks: 0
    D.FilesState { id: state; persistViewPreferences: false }
    function check(ok, message) { if (!ok) throw Error(message); }
    Timer {
        interval: 60
        repeat: true
        running: true
        onTriggered: {
            try {
                if (++ticks > 80) throw Error("checkpoint timed out");
                if (phase === 0) { check(!state.listingCurrent && !state.listingComplete, "uninitialized authority"); state.navigate("/complete"); check(!state.listingCurrent, "new request retained authority"); phase++; return; }
                if (phase === 1) { if (state.busy) return; check(state.listingCurrent && state.listingComplete, "complete reply not committed"); state.navigate("/partial"); phase++; return; }
                if (phase === 2) { if (state.busy) return; check(state.listingCurrent && !state.listingComplete, "limited reply allowed metadata pruning"); state.navigate("/unreadable"); phase++; return; }
                if (phase === 3) { if (state.busy) return; check(state.listingCurrent && !state.listingComplete, "unreadable reply allowed metadata pruning"); state.navigate("/error"); phase++; return; }
                if (phase === 4) { if (state.busy) return; check(!state.listingCurrent && !state.listingComplete, "error became authoritative"); state.navigate("/stale"); phase++; return; }
                if (phase === 5) { if (ticks < 10) return; check(state.busy && !state.listingCurrent, "stale reply committed"); state.navigate("/complete"); phase++; return; }
                if (phase === 6) { if (state.busy) return; check(state.listingCurrent && state.listingComplete, "fresh reply failed recovery"); state.path = "/another"; check(!state.listingCurrent, "path changed without invalidating authority"); console.log("FILES_CHECKPOINT_PASS"); Qt.quit(); }
            } catch (error) { console.error("FILES_CHECKPOINT_FAIL " + error); Qt.exit(1); }
        }
    }
}

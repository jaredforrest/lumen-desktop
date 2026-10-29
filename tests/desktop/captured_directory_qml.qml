import QtQuick
import Quickshell
import "assets/shell" as D

ShellRoot {
    D.FilesState { id: state; persistViewPreferences: false; active: false }
    property int step: 0
    property string initial: __INITIAL__
    property string target: __TARGET__
    property var expected: __FACTS__
    function check(ok, message) { if (!ok) throw Error(message); }
    Timer {
        interval: 50; repeat: true; running: true
        onTriggered: {
            try {
                if (step === 0) { state.navigate(initial); step++; return; }
                if (state.busy || !state.ready) return;
                if (step === 1) {
                    check(state.path === initial && state.rows.length === 1, "initial list " + JSON.stringify({path:state.path,rows:state.rows,error:state.error}));
                    state.navigate(target, undefined, false, expected); step++; return;
                }
                if (step === 2) {
                    check(state.error !== "", "captured replacement accepted");
                    check(state.path === initial && state.rows.length === 1 && state.rows[0].name === "retained.txt", "wrong rows committed");
                    check(state.history.length === 1, "failed navigation committed history");
                    console.log("PASS actual Files expected-directory request and rejection preserve rows/path/history");
                    Qt.quit();
                }
            } catch (error) {
                console.error("FAIL " + error); Qt.exit(1);
            }
        }
    }
}

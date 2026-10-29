import QtQuick
import Quickshell
import "assets/shell" as D

ShellRoot {
    D.DisplaySpacesState { id: policy }
    property int step: 0
    property int changes: 0
    property int expected: 0
    property int turns: 0
    Connections { target: policy; function onStateChanged() { changes++; } }
    property var validFrame: ({protocolVersion: 1, nativeSupported: true,
        session: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", generation: "2", available: true,
        applied: true, separateDisplays: false, independent: false, primary: "A",
        primaryDisplay: null,
        outputs: [{name: "A", label: "Display", identity: {kind: "connector", name: "A",
            make: null, model: null, serial: null}}], error: ""})
    function send(value) {
        expected = changes + 1;
        policy.observer.write((typeof value === "string" ? value : JSON.stringify(value)) + "\n");
    }
    function check(value, message) { if (!value) throw Error(message); }
    Timer {
        interval: 50
        repeat: true
        running: true
        onTriggered: {
            try {
                turns++;
                check(turns < 120, "wire acknowledgement deadline");
                if (step === 0) {
                    if (!policy.observer.running) return;
                    check(policy.routing.mode === "independent", "startup hid Dock");
                    send(validFrame); step++; return;
                }
                if (changes < expected) return;
                if (step === 1) {
                    check(policy.state.applied && policy.routing.mode === "main", "valid frame refused");
                    send("{broken"); step++; return;
                }
                if (step === 2) {
                    check(!policy.state.applied && policy.routing.mode === "main", "malformed JSON lost routing or kept authority");
                    send(Object.assign({}, validFrame, {generation: "1"})); step++; return;
                }
                if (step === 3) {
                    check(!policy.state.applied && policy.lastAccepted.generation === "2", "stale generation accepted");
                    send(Object.assign({}, validFrame, {independent: true})); step++; return;
                }
                if (step === 4) {
                    check(!policy.state.applied && policy.routing.mode === "main" && policy.lastAccepted.generation === "2", "contradictory authority changed routing");
                    send(Object.assign({}, validFrame, {session: "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb", generation: "0"})); step++; return;
                }
                if (step === 5) {
                    check(policy.state.applied && policy.lastAccepted.generation === "0", "new session refused");
                    send(Object.assign({}, validFrame, {nativeSupported: false, session: null,
                        generation: null, available: false, applied: false, independent: true,
                        primary: "", error: "Upgrade required"})); step++; return;
                }
                check(!policy.state.applied && policy.lastAccepted.nativeSupported === false &&
                    policy.routing.mode === "independent", "legacy fallback claimed authority or lost routing");
                console.log("DISPLAY_WIRE_PASS"); Qt.quit();
            } catch (error) {
                console.error("DISPLAY_WIRE_FAIL " + error); Qt.quit();
            }
        }
    }
}

import QtQuick
import QtQuick.Controls
import Quickshell

ShellRoot {
    property var sent: []
    property int changes: 0
    TrashActionsState {
        id: state
        command: ["false"]
        onChanged: changes++
        function send(action, data) { sent = sent.concat([{action: action, data: data}]); return ++state.sequence; }
    }
    ApplicationWindow {
        id: window
        width: 600
        height: 500
        visible: true
        TrashConfirmation { id: confirmation; state: state }
        TrashResults { state: state }
    }
    Timer {
        interval: 100
        running: true
        onTriggered: {
            try {
                state.prepare("empty", []);
                if (sent[0].action !== "trash-prepare" || "items" in sent[0].data) throw Error("empty capture accepted caller paths");
                const retired = state.prepareId;
                state.retire();
                state.receive(JSON.stringify({action: "trash-prepare", id: retired, token: "stale", names: ["wrong"], count: 1}));
                if (state.confirmation || sent[sent.length - 1].action !== "trash-cancel") throw Error("late preparation became actionable");
                state.prepare("selected", [{path: "/synthetic/Trash/files/a", identity: {device: "3", inode: "1152921504606846977"}}]);
                const captured = state.prepareId;
                state.receive(JSON.stringify({action: "trash-prepare", id: captured, token: "captured-token", names: ["Literal <name> é"], count: 1}));
                if (!confirmation.visible || state.confirmation.token !== "captured-token") throw Error("confirmation not opened");
                state.confirm();
                const confirm = sent.find(item => item.action === "trash-confirm");
                if (!confirm || JSON.stringify(confirm.data) !== JSON.stringify({token: "captured-token"}) || state.confirmation) throw Error("confirmation retargeted or retained");
                state.receive(JSON.stringify({action: "trash-confirm", busy: false, operations: [{state: "complete"}], history: []}));
                if (changes !== 1 || state.busy || state.waiting) throw Error("immediate completion did not refresh consumer");
                console.log("PASS captured Trash confirmation");
            } catch (error) {
                console.error("FAIL " + error);
            }
            Qt.quit();
        }
    }
}

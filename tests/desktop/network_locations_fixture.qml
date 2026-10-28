import QtQuick
import Quickshell

ShellRoot {
    id: root
    property int phase: 0
    property int ticks: 0
    FloatingWindow {
        visible: true
        implicitWidth: 700
        implicitHeight: 800
        Loader {
            id: state
            Component.onCompleted: setSource("LUMEN_ASSETS/NetworkLocationsState.qml", {command: ["LUMEN_PROVIDER_BINARY"], active: true})
            onLoaded: pane.setSource("LUMEN_ASSETS/NetworkLocationsPane.qml", {service: item})
        }
        Loader { id: pane; width: parent.width - 32; x: 16; y: 16 }
    }
    function check(value, message) { if (!value) throw Error(message); }
    Timer {
        interval: 80
        running: true
        repeat: true
        onTriggered: {
            try {
                if (++ticks > 100) throw Error("Network fixture timeout phase " + phase);
                if (!state.item || !pane.item) return;
                const service = state.item;
                if (phase === 0) {
                    if (!service.ready) return;
                    check(service.available && service.protocols[0] === "sftp", "Provider capability was not read");
                    service.connectServer("sftp://fixture/root"); phase = 1; return;
                }
                if (phase === 1) {
                    if (service.busy || service.rows.length !== 2) return;
                    check(service.current.state === "connected" && service.localPath === "", "Missing FUSE path was invented");
                    if (Quickshell.env("LUMEN_NETWORK_SCREENSHOT") && ticks < 15) {
                        Quickshell.execDetached(["grim", Quickshell.env("LUMEN_NETWORK_SCREENSHOT")]); phase = 10; return;
                    }
                }
                if (phase === 10) { if (ticks < 15) return; phase = 1; }
                if (phase === 1) {
                    service.browseId = 300;
                    service.receive({id: 299, rows: [{name: "stale", uri: "bad"}], connectionId: service.selectedId});
                    check(service.rows[0].name === "Folder", "Stale browse response changed rows");
                    service.browseId = 0;
                    service.connectServer("sftp://fixture/ask"); phase = 2; return;
                }
                if (phase === 2) {
                    if (!service.current || service.current.state !== "authenticating") return;
                    check(service.busy, "Authentication did not remain cancellable");
                    service.cancel(); phase = 3; return;
                }
                if (phase === 3) {
                    if (service.current && service.current.state !== "disconnected") return;
                    check(!service.busy, "Cancel left UI busy");
                    service.active = false;
                    service.browseId = 400;
                    service.receive({id: 400, rows: [{name: "late", uri: "bad"}]});
                    check(service.rows.length === 0, "Closed view repopulated rows");
                    console.log("NETWORK_QT_PASS"); Qt.quit();
                }
            } catch (error) { console.error("NETWORK_QT_FAIL " + error); Qt.quit(); }
        }
    }
}

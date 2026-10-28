import QtQuick
import Quickshell

ShellRoot {
    property int ticks: 0
    QtObject { id: quick; function show(path, owner) {} }
    QtObject { id: shell; property var quickLookWindow: quick; function openFiles(path) {} }
    QtObject {
        id: service
        property var rows: []
        property bool busy: false
        property string error: ""
        property string warning: ""
        property var calls: []
        signal informationReady(var details)
        function startJob(operation, paths, destination, conflict) {
            calls = calls.concat([{operation: operation, paths: paths, destination: destination, conflict: conflict}]);
        }
    }
    FloatingWindow {
        visible: true
        implicitWidth: 1200
        implicitHeight: 900
        Loader {
            id: desktop
            anchors.fill: parent
            Component.onCompleted: setSource("LUMEN_ASSETS/DesktopFiles.qml", {shell: shell, service: service, resolveDirectory: false, directory: "/fixture", active: false})
        }
    }
    function check(value, message) { if (!value) throw Error(message); }
    Timer {
        interval: 100
        running: true
        repeat: true
        onTriggered: {
            try {
                if (++ticks > 30) throw Error("Drop fixture timeout");
                if (!desktop.item) return;
                const item = desktop.item;
                check(!item.proposeCopy(["https://invalid.test/file"], "/fixture/Folder"), "Remote offer accepted");
                check(service.calls.length === 0, "Rejected offer started job");
                check(item.proposeCopy(["file:///fixture/a%20b", "file:///fixture/a%20b"], "/fixture/Folder"), "Local offer rejected");
                check(item.dropConfirmationVisible && item.pendingDropPaths.length === 1, "Prompt missing or duplicates retained");
                check(item.error === "", "Successful offer retained stale error");
                check(service.calls.length === 0, "Copy started before confirmation");
                check(!item.proposeCopy(["file:///fixture/b"], "relative"), "Relative destination accepted");
                check(!item.dropConfirmationVisible && item.pendingDropPaths.length === 0, "Rejected replacement retained actionable pending copy");
                item.confirmDrop(); check(service.calls.length === 0, "Cancelled offer remained actionable");
                check(item.proposeCopy(["file:///fixture/a%20b"], "/fixture/Folder"), "Replacement offer rejected");
                item.confirmDrop(); check(service.calls.length === 1, "Confirmed copy not submitted");
                const job = service.calls[0];
                check(job.operation === "copy" && job.conflict === "skip" && job.destination === "/fixture/Folder" && job.paths[0] === "/fixture/a b", "Copy contract changed");
                check(!item.dropConfirmationVisible && item.pendingDropPaths.length === 0, "Prompt state retained");
                console.log("DESKTOP_DROP_QT_PASS"); Qt.quit();
            } catch (error) {
                console.error("DESKTOP_DROP_QT_FAIL " + error); Qt.quit();
            }
        }
    }
}

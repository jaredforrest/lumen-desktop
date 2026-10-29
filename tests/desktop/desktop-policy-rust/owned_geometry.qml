import QtQuick
import Quickshell
import Quickshell.Io
import "assets/shell" as D

ShellRoot {
    id: fixture
    property string kind: Quickshell.env("OWNED_FIXTURE_KIND") || "Settings"
    FloatingWindow {
        id: owned
        visible: true
        title: fixture.kind === "Settings" ? "System Settings" : "Calendar"
        implicitWidth: 640
        implicitHeight: 480
        minimumSize: Qt.size(320, 240)
        function fitScreen() { geometry.fitWorkspace(32, 32); }
        D.WindowGeometry {
            id: geometry
            anchors.fill: parent
            ownerWindow: owned
            persistenceKey: fixture.kind === "Settings" ? "settings.main" : "calendar.main"
        }
        Component.onCompleted: fitScreen()
    }
    IpcHandler { target: "ownedGeometryAudit"
        function status(): string {
            return JSON.stringify({title: owned.title, visible: owned.visible,
                screen: owned.screen ? owned.screen.name : "", width: owned.width,
                height: owned.height, completed: geometry.completedRequests, busy: geometry.busy,
                error: geometry.error, persistenceSupported: geometry.persistenceSupported,
                persistenceStatus: geometry.persistenceStatus, result: geometry.result});
        }
        function hide(): void { owned.visible = false; }
        function show(): void { owned.visible = true; owned.fitScreen(); }
    }
}

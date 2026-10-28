import QtQuick
import Quickshell
import "LUMEN_ASSETS" as D

ShellRoot {
    Component.onCompleted: D.NearbySharingState.refresh()
    Timer {
        interval: 300
        running: true
        onTriggered: {
            if (D.NearbySharingState.label !== "Nearby Sharing" || D.NearbySharingState.configured || D.NearbySharingState.available || D.NearbySharingState.active || D.NearbySharingState.canSend)
                console.error("FAIL unexpected sharing state");
            else
                console.log("PASS unconfigured sharing remains inactive");
            Qt.quit();
        }
    }
}

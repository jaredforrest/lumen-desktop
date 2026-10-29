import QtQuick
import Quickshell
import Quickshell.Wayland

ShellRoot {
    Variants {
        model: Quickshell.screens
        PanelWindow {
            required property var modelData
            screen: modelData
            anchors { top: true; left: true; right: true }
            implicitHeight: 40
            exclusiveZone: 40
            color: "#242424"
            WlrLayershell.layer: WlrLayer.Top
        }
    }
}

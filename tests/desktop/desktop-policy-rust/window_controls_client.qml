import QtQuick
import Quickshell

ShellRoot {
    FloatingWindow {
        visible: true
        title: Quickshell.env("WINDOW_CONTROLS_TITLE")
        implicitWidth: 400
        implicitHeight: 240
        TextEdit { anchors.fill: parent; focus: true; text: "Synthetic document" }
    }
}

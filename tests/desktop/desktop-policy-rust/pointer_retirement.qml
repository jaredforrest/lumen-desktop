import QtQuick
import QtQuick.Window
import Quickshell
import Quickshell.Io

ShellRoot {
    Window {
        id: a
        title: "Pointer A"
        visible: true
        width: 300
        height: 220
        Rectangle { anchors.fill: parent; color: "#394b60" }
        MouseArea {
            id: ma
            anchors.fill: parent
            hoverEnabled: true
            acceptedButtons: Qt.AllButtons
            property int releases: 0
            property int moves: 0
            onReleased: releases++
            onPositionChanged: if (pressedButtons) moves++
        }
    }
    Window {
        id: b
        title: "Pointer B"
        visible: true
        width: 300
        height: 220
        Rectangle { anchors.fill: parent; color: "#506f45" }
        MouseArea {
            id: mb
            anchors.fill: parent
            hoverEnabled: true
            acceptedButtons: Qt.AllButtons
            property int releases: 0
            property int moves: 0
            onReleased: releases++
            onPositionChanged: if (pressedButtons) moves++
        }
    }
    IpcHandler {
        target: "fixture"
        function status(): string {
            return JSON.stringify({A:{hovered:ma.containsMouse,buttons:Number(ma.pressedButtons),releases:ma.releases,moves:ma.moves},
                B:{hovered:mb.containsMouse,buttons:Number(mb.pressedButtons),releases:mb.releases,moves:mb.moves}});
        }
        function hideA(): void { a.visible = false; }
        function showA(): void { a.visible = true; }
        function hideB(): void { b.visible = false; }
    }
}

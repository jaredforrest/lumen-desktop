import QtQuick
import QtQuick.Controls
ScrollBar {
    id: control
    padding: 2
    visible: size < 1.0
    readonly property bool alwaysVisible: Theme.preferences.scrollbars === "always"
    opacity: alwaysVisible || active || hovered || pressed || linger.running ? 1 : 0
    onActiveChanged: if (active) linger.restart()
    onPositionChanged: linger.restart()
    Timer { id:linger; interval:1200 }
    Behavior on opacity { NumberAnimation {duration:Theme.preferences.reduceMotion?0:140} }
    implicitWidth: 10
    implicitHeight: 10
    minimumSize: 0.08
    contentItem: Rectangle {
        implicitWidth: 6; implicitHeight: 6
        radius: 3
        color: Theme.dark ? "#949498" : "#89898e"
        opacity: control.pressed ? 0.9 : control.hovered ? 0.75 : 0.48
    }
    background: Rectangle {
        radius: 5
        color: Theme.colors.field
        opacity: control.hovered || control.pressed ? 0.6 : 0
    }
}

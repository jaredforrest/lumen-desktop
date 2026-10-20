import QtQuick
Rectangle {
    required property bool focused
    property real inset: 0
    anchors.fill: parent
    anchors.margins: inset
    visible: focused
    color: "transparent"
    radius: 6
    border.width: 2
    border.color: Theme.colors.text
    Accessible.ignored: true
}

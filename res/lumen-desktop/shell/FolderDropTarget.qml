import QtQuick
Item {
    id:root
    required property string folder
    required property var coordinator
    Component.onCompleted:coordinator.registerTarget(root)
    Component.onDestruction:if(coordinator)coordinator.unregisterTarget(root)
    Rectangle {anchors.fill:parent;radius:5;color:Theme.selection;opacity:.55;visible:root.coordinator.hoverTarget===root;border.width:2;border.color:Theme.colors.accent}
}

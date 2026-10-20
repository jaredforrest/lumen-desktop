import QtQuick
import QtQuick.Controls
AbstractButton {
    id:root
    required property var state
    property bool selected:false
    objectName:"capture-indicator"
    visible:!!state && (state.audio || state.video)
    implicitWidth:28;implicitHeight:24
    Accessible.name:(state && state.audio?"Audio ":"")+(state && state.video?"video ":"")+"capture observed. Open capture observations"
    background:Rectangle {radius:6;color:root.selected || root.hovered?Theme.colors.hover:"transparent";border.width:root.activeFocus?1:0;border.color:Theme.colors.accent}
    contentItem:Item {
        Row {anchors.centerIn:parent;spacing:3
            Rectangle {visible:!!root.state && root.state.audio;width:6;height:6;radius:3;color:Theme.warningText}
            Rectangle {visible:!!root.state && root.state.video;width:6;height:6;radius:3;color:Theme.successText}
        }
    }
}

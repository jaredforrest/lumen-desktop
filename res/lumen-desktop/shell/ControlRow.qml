import QtQuick
import QtQuick.Controls
AbstractButton {
    id: root
    property string detail: ""
    property bool selected: false
    implicitHeight: Math.max(detail?46:32,labels.implicitHeight+14)
    Accessible.name: text + (detail ? ", " + detail : "")
    contentItem: Column {
        id:labels
        spacing: 2
        PanelLabel {textFormat:Text.PlainText;width:parent.width;text:(root.selected?"✓  ":"")+root.text;wrapMode:Text.Wrap;elide:Text.ElideNone;opacity:root.enabled?1:0.5}
        PanelLabel {textFormat:Text.PlainText;width:parent.width;visible:text!=="";text:root.detail;wrapMode:Text.Wrap;elide:Text.ElideNone;font.pixelSize:Theme.textSize(10);color:Theme.colors.muted}
    }
    leftPadding: 8; rightPadding: 8; topPadding: 7; bottomPadding: 7
    background: Rectangle {radius:7;color:root.hovered || root.activeFocus?Theme.colors.hover:"transparent"}
}

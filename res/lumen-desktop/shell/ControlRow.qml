import QtQuick
import QtQuick.Controls
AbstractButton {
    id: root
    property string detail: ""
    property bool selected: false
    implicitHeight: detail ? 46 : 32
    Accessible.name: text + (detail ? ", " + detail : "")
    contentItem: Column {
        spacing: 2
        PanelLabel {textFormat:Text.PlainText;width:parent.width;text:(root.selected?"✓  ":"")+root.text;elide:Text.ElideRight;opacity:root.enabled?1:0.5}
        PanelLabel {textFormat:Text.PlainText;width:parent.width;visible:text!=="";text:root.detail;elide:Text.ElideRight;font.pixelSize:10;color:Theme.colors.muted}
    }
    leftPadding: 8; rightPadding: 8; topPadding: 7; bottomPadding: 7
    background: Rectangle {radius:7;color:root.hovered || root.activeFocus?Theme.colors.hover:"transparent"}
}

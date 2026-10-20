import QtQuick
import QtQuick.Controls
AbstractButton {
    id:root
    property string detail:""
    checkable:true
    implicitHeight:Math.max(detail?58:44,labels.implicitHeight+20)
    Accessible.name:text
    Accessible.role:Accessible.CheckBox
    leftPadding:14;rightPadding:14
    contentItem:Item {
        Column {
            id:labels
            anchors.left:parent.left;anchors.right:switchTrack.left;anchors.rightMargin:18;anchors.verticalCenter:parent.verticalCenter;spacing:3
            PanelLabel {width:parent.width;text:root.text;wrapMode:Text.Wrap;elide:Text.ElideNone;opacity:root.enabled?1:0.5}
            PanelLabel {width:parent.width;visible:root.detail!=="";text:root.detail;color:Theme.colors.muted;font.pixelSize:Theme.textSize(11);wrapMode:Text.WordWrap}
        }
        Rectangle {
            id:switchTrack;anchors.right:parent.right;anchors.verticalCenter:parent.verticalCenter;width:32;height:20;radius:10
            color:root.checked?Theme.colors.accent:Theme.colors.field;opacity:root.enabled?1:0.4
            border.color:root.activeFocus?Theme.colors.text:"transparent";border.width:root.activeFocus?2:0
            Rectangle {x:root.checked?14:2;y:2;width:16;height:16;radius:8;color:"white";border.width:1;border.color:Theme.textOnOpaqueFill(switchTrack.color)}
        }
    }
    background:Rectangle {radius:10;color:Theme.colors.surface;border.color:root.activeFocus?Theme.colors.accent:Theme.hairline}
}

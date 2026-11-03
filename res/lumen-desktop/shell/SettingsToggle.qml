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
            id:switchTrack;anchors.right:parent.right;anchors.verticalCenter:parent.verticalCenter;width:36;height:22;radius:11
            color:root.checked?Theme.selectionFill:Theme.dark?"#636366":"#b9b9bf";opacity:root.enabled?1:0.4
            border.color:root.activeFocus?Theme.colors.text:"transparent";border.width:root.activeFocus?2:0
            Rectangle {x:root.checked?16:2;y:2;width:18;height:18;radius:9;color:"white";border.width:0}
        }
    }
    background:Rectangle {radius:10;color:Theme.colors.surface;border.color:root.activeFocus?Theme.colors.accent:Theme.hairline}
}

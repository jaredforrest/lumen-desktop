import QtQuick
import QtQuick.Controls
Item {
    id:root
    property string title:""
    property bool collapsed:false
    property Component accessory
    default property alias content:body.data
    signal collapseRequested(bool value)
    implicitHeight:heading.height+(collapsed?0:body.height+3)
    height:implicitHeight
    AbstractButton {id:heading;x:0;y:0;width:parent.width-(accessoryLoader.active?28:0);height:25;checkable:true;checked:!root.collapsed
        Accessible.name:root.title+(root.collapsed?", collapsed":", expanded");Accessible.description:root.collapsed?"Expand group":"Collapse group"
        onClicked:root.collapseRequested(!root.collapsed)
        contentItem:PanelLabel {leftPadding:8;text:(root.collapsed?"›  ":"⌄  ")+root.title;verticalAlignment:Text.AlignVCenter;font.pixelSize:11;font.weight:Font.DemiBold;color:Theme.colors.muted}
        background:Rectangle {radius:5;color:parent.hovered || parent.activeFocus?Theme.colors.hover:"transparent";border.color:parent.activeFocus?Theme.colors.accent:"transparent"}
    }
    Loader {id:accessoryLoader;anchors.right:parent.right;width:25;height:25;active:root.accessory!==null;sourceComponent:root.accessory}
    Column {id:body;x:0;y:heading.height+3;width:parent.width;spacing:3;visible:!root.collapsed}
}

import QtQuick
PanelSurface {
    id: tile
    signal clicked()
    property bool active: false
    property bool toggle: false
    property string label
    property string subtitle: unavailable && label ? "Unavailable" : ""
    property string symbol
    property url iconSource:""
    property bool grouped:false
    property bool unavailable: true
    property string accessibleLabel: label || symbol
    implicitHeight:Math.max(grouped?48:62,labels.implicitHeight+(grouped?16:24))
    radius:grouped?0:18
    color:grouped?"transparent":Theme.colors.surface
    border.width:grouped?0:1
    layer.enabled:false
    Row {id:tileBody;width:tile.width-16;x:parent.label?8:(parent.width-width)/2;anchors.verticalCenter:parent.verticalCenter;spacing:6;opacity:parent.unavailable?0.55:1
        Rectangle {width:30;height:30;radius:15;color:tile.active?Theme.colors.accent:parent.parent.label?Theme.colors.surface:"transparent"
            SystemGlyph {anchors.centerIn:parent;width:18;height:18;name:parent.parent.parent.symbol;imageSource:tile.iconSource;ink:tile.active?Theme.colors.selectedText:Theme.colors.text}
        }
        Column {id:labels;width:Math.max(0,tileBody.width-36);visible:parent.parent.label!=="";anchors.verticalCenter:parent.verticalCenter;spacing:1
            PanelLabel {width:parent.width;text:parent.parent.parent.label;wrapMode:Text.WordWrap;elide:Text.ElideNone;font.pixelSize:Theme.textSize(12);font.weight:Font.DemiBold}
            PanelLabel {width:parent.width;visible:text!=="";text:parent.parent.parent.subtitle;wrapMode:Text.Wrap;elide:Text.ElideNone;font.pixelSize:Theme.textSize(10);color:Theme.colors.muted}
        }
    }
    MouseArea {anchors.fill:parent;enabled:!tile.unavailable;onClicked:tile.clicked()}
    enabled:!unavailable
    activeFocusOnTab:!unavailable
    Keys.onReturnPressed: if(!unavailable)clicked()
    Keys.onSpacePressed: if(!unavailable)clicked()
    Accessible.role: toggle?Accessible.CheckBox:Accessible.Button
    Accessible.checkable: toggle
    Accessible.checked: toggle && active
    Accessible.onPressAction: if(!unavailable)clicked()
    Accessible.name:accessibleLabel+(subtitle?", "+subtitle:"")+(unavailable?" — unavailable":"")
}

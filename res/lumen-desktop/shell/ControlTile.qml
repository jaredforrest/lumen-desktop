import QtQuick
PanelSurface {
    id: tile
    signal clicked()
    property bool active: false
    property bool toggle: false
    property string label
    property string subtitle: unavailable && label ? "Unavailable" : ""
    property string symbol
    property bool unavailable: true
    property string accessibleLabel: label || symbol
    implicitHeight:Math.max(62,labels.implicitHeight+24)
    radius:Math.min(31,height/2)
    Row {id:tileBody;width:tile.width-24;x:parent.label?12:(parent.width-width)/2;anchors.verticalCenter:parent.verticalCenter;spacing:10;opacity:parent.unavailable?0.55:1
        Rectangle {width:30;height:30;radius:15;color:tile.active?Theme.colors.accent:parent.parent.label?Theme.colors.surface:"transparent"
            SystemGlyph {anchors.centerIn:parent;name:parent.parent.parent.symbol;ink:tile.active?Theme.colors.selectedText:Theme.colors.text}
        }
        Column {id:labels;width:Math.max(0,tileBody.width-40);visible:parent.parent.label!=="";anchors.verticalCenter:parent.verticalCenter;spacing:1
            PanelLabel {width:parent.width;text:parent.parent.parent.label;wrapMode:Text.Wrap;elide:Text.ElideNone;font.pixelSize:Theme.textSize(12);font.weight:Font.DemiBold}
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

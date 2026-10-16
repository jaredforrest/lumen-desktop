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
    radius:height/2
    Row {x:parent.label?12:(parent.width-width)/2;anchors.verticalCenter:parent.verticalCenter;spacing:10;opacity:parent.unavailable?0.55:1
        Rectangle {width:30;height:30;radius:15;color:tile.active?Theme.colors.accent:parent.parent.label?Theme.colors.surface:"transparent"
            SystemGlyph {anchors.centerIn:parent;name:parent.parent.parent.symbol;ink:tile.active?Theme.colors.selectedText:Theme.colors.text}
        }
        Column {visible:parent.parent.label!=="";anchors.verticalCenter:parent.verticalCenter;spacing:1
            PanelLabel {text:parent.parent.parent.label;font.pixelSize:12;font.weight:Font.DemiBold}
            PanelLabel {visible:text!=="";text:parent.parent.parent.subtitle;font.pixelSize:10;color:Theme.colors.muted}
        }
    }
    MouseArea {anchors.fill:parent;enabled:!tile.unavailable;onClicked:tile.clicked()}
    activeFocusOnTab: !unavailable
    Keys.onReturnPressed: if(!unavailable)clicked()
    Keys.onSpacePressed: if(!unavailable)clicked()
    Accessible.role: toggle?Accessible.CheckBox:Accessible.Button
    Accessible.checkable: toggle
    Accessible.checked: toggle && active
    Accessible.onPressAction: if(!unavailable)clicked()
    Accessible.name:accessibleLabel+(subtitle?", "+subtitle:"")+(unavailable?" — unavailable":"")
}

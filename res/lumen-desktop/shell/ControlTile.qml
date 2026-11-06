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
    property bool vertical:false
    property bool unavailable: true
    property string accessibleLabel: label || symbol
    implicitHeight:Math.max(vertical?92:grouped?48:62,labels.implicitHeight+(vertical?54:grouped?16:24))
    radius:grouped?0:18
    color:grouped?"transparent":Theme.colors.surface
    border.width:grouped?0:1
    layer.enabled:false
    Item {id:tileBody;width:tile.width-16;height:tile.vertical?Math.max(64,30+6+labels.implicitHeight):Math.max(30,labels.implicitHeight);x:8;anchors.verticalCenter:parent.verticalCenter;opacity:tile.unavailable?0.55:1
        Rectangle {x:tile.vertical?(parent.width-width)/2:0;y:tile.vertical?0:(parent.height-height)/2;width:30;height:30;radius:15;color:tile.active?Theme.colors.accent:tile.label?Theme.colors.surface:"transparent"
            SystemGlyph {anchors.centerIn:parent;width:18;height:18;name:parent.parent.parent.symbol;imageSource:tile.iconSource;ink:tile.active?Theme.colors.selectedText:Theme.colors.text}
        }
        Column {id:labels;x:tile.vertical?0:36;y:tile.vertical?36:(tileBody.height-height)/2;width:Math.max(0,tileBody.width-(tile.vertical?0:36));visible:tile.label!=="";spacing:2
            PanelLabel {width:parent.width;text:tile.label;wrapMode:Text.WordWrap;horizontalAlignment:tile.vertical?Text.AlignHCenter:Text.AlignLeft;elide:Text.ElideNone;font.pixelSize:Theme.textSize(12);font.weight:Font.DemiBold}
            PanelLabel {width:parent.width;visible:text!=="";text:tile.subtitle;wrapMode:Text.Wrap;horizontalAlignment:tile.vertical?Text.AlignHCenter:Text.AlignLeft;elide:Text.ElideNone;font.pixelSize:Theme.textSize(10);color:Theme.colors.muted}
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

import QtQuick
import QtQuick.Effects
Rectangle {
    id: button
    property string text
    property string iconName: ""
    property int textSize: 13
    property bool strong: false
    property bool selected: false
    signal clicked()
    signal entered()
    implicitWidth: (iconName ? 18 : label.implicitWidth) + 20; implicitHeight: 24
    clip:true
    radius: 6; color: (selected || mouse.containsMouse) ? Theme.colors.hover : "transparent"
    Text { id: label; anchors.centerIn: parent;width:parent.width-20;horizontalAlignment:Text.AlignHCenter;elide:Text.ElideRight;text: button.text; color: Theme.colors.text; font.family: Theme.font; font.pixelSize: button.textSize; font.weight: button.strong ? Font.DemiBold : Font.Normal }
    Image { layer.enabled: GraphicsInfo.api !== GraphicsInfo.Software; layer.effect: MultiEffect { colorization: 1; colorizationColor: Theme.colors.text } anchors.centerIn: parent; width:16; height:16; visible:button.iconName!=="" && GraphicsInfo.api !== GraphicsInfo.Software; source:button.iconName ? "icons/"+button.iconName+".svg" : ""; sourceSize.width:44; sourceSize.height:44 }
    SystemGlyph {anchors.centerIn:parent;width:16;height:16;visible:button.iconName!=="" && GraphicsInfo.api===GraphicsInfo.Software;name:button.iconName==="sound"?"speaker":button.iconName;ink:Theme.colors.text}
    MouseArea { id: mouse; anchors.fill: parent; hoverEnabled: true; onClicked: button.clicked(); onEntered:button.entered() }
    Accessible.role: Accessible.Button
    Accessible.name: iconName || text
    Accessible.onPressAction: clicked()
}

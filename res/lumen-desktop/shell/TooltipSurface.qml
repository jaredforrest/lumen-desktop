import QtQuick
Rectangle {
    id:root
    property string text:""
    property real maximumWidth:360
    property real maximumHeight:480
    readonly property alias label:label
    width:Math.min(Math.max(1,maximumWidth),Math.max(1,label.implicitWidth+14))
    height:Math.min(Math.max(1,maximumHeight),label.implicitHeight+14)
    radius:6;color:Theme.colors.background;border.color:Theme.outline;border.width:1
    clip:true
    Text {id:label;x:7;y:7;width:Math.max(0,parent.width-14);height:Math.max(0,parent.height-14);text:root.text;textFormat:Text.PlainText;wrapMode:Text.Wrap;elide:Text.ElideRight;color:Theme.colors.text;font.family:Theme.font;font.pixelSize:Theme.textRole("metadata")}
}

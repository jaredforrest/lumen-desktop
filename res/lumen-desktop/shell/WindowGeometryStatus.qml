import QtQuick
Rectangle {
    id:root
    objectName:"owned-window-placement-footer"
    property string text:""
    anchors.left:parent.left
    anchors.right:parent.right
    anchors.bottom:parent.bottom
    height:root.text.length>0?Math.ceil(message.implicitHeight)+20:0
    visible:height>0
    color:Theme.colors.background
    z:100
    Rectangle {width:parent.width;height:1;color:Theme.hairline}
    StatusMessage {
        id:message
        objectName:"owned-window-placement-status"
        x:10;y:10;width:Math.max(1,parent.width-20)
        text:root.text
        severity:"error"
    }
}

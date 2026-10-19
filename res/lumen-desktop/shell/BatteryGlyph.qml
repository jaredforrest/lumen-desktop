import QtQuick
Item {
    id:root
    property real percentage:0
    property bool charging:false
    property color ink:Theme.colors.text
    implicitWidth:24;implicitHeight:14
    Rectangle {x:1;y:2;width:19;height:10;radius:2;color:"transparent";border.width:1.2;border.color:root.ink
        Rectangle {x:2;y:2;width:15*Math.max(0,Math.min(1,root.percentage/100));height:6;radius:0.5;color:root.ink}
    }
    Rectangle {x:21;y:5;width:2;height:4;radius:0.7;color:root.ink}
    Canvas {id:bolt;anchors.fill:parent;visible:root.charging
        onVisibleChanged:requestPaint()
        Connections {target:root;function onInkChanged(){bolt.requestPaint();}}
        onPaint:{const c=getContext("2d");c.reset();c.beginPath();c.moveTo(12,1);c.lineTo(8,7);c.lineTo(11,7);c.lineTo(10,13);c.lineTo(16,5);c.lineTo(12,5);c.closePath();c.lineWidth=2;c.strokeStyle=Theme.colors.background;c.stroke();c.fillStyle=root.ink;c.fill();}
    }
}

import QtQuick
import QtQuick.Controls
Slider {
    id:root
    property string leadingGlyph:""
    property string trailingGlyph:""
    property bool compact:true
    implicitHeight:compact?24:30
    leftPadding:leadingGlyph?24:8
    rightPadding:trailingGlyph?26:8
    SystemGlyph {visible:root.leadingGlyph!=="";x:0;anchors.verticalCenter:parent.verticalCenter;name:root.leadingGlyph;width:14;height:14;ink:Theme.colors.muted}
    SystemGlyph {visible:root.trailingGlyph!=="";anchors.right:parent.right;anchors.verticalCenter:parent.verticalCenter;name:root.trailingGlyph;width:18;height:18;ink:Theme.colors.muted}
    background:Rectangle {x:root.leftPadding;y:(root.height-height)/2;width:root.availableWidth;height:6;radius:3;color:Theme.hairline;opacity:root.enabled?1:0.4
        Rectangle {width:root.visualPosition*parent.width;height:parent.height;radius:3;color:Theme.colors.accent}
    }
    handle:Rectangle {x:root.leftPadding+root.visualPosition*(root.availableWidth-width);y:(root.height-height)/2;width:14;height:14;radius:7;color:Theme.colors.surface;opacity:root.enabled?1:0.4;border.width:root.activeFocus?2:1;border.color:root.activeFocus?Theme.colors.accent:Theme.outline}
}

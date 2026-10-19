import QtQuick
Item {
    id:root
    property int value:190
    property int minimum:120
    property int maximum:360
    signal previewRequested(int value)
    signal committed(int value)
    signal cancelled()
    function bounded(value){return Math.max(minimum,Math.min(maximum,Math.round(value)));}
    function adjust(delta){committed(bounded(value+delta));}
    width:8;activeFocusOnTab:true
    Accessible.role:Accessible.Slider;Accessible.name:"Sidebar width";Accessible.description:value+" pixels; use Left or Right to resize"
    Accessible.onIncreaseAction:adjust(10)
    Accessible.onDecreaseAction:adjust(-10)
    Keys.onPressed:event=>{if(event.key===Qt.Key_Left){adjust(-10);event.accepted=true;}else if(event.key===Qt.Key_Right){adjust(10);event.accepted=true;}else if(event.key===Qt.Key_Home){committed(minimum);event.accepted=true;}else if(event.key===Qt.Key_End){committed(maximum);event.accepted=true;}else if(event.key===Qt.Key_Escape){input.gestureCancelled=true;cancelled();event.accepted=true;}else event.accepted=false;}
    Rectangle {anchors.horizontalCenter:parent.horizontalCenter;width:root.activeFocus?2:1;height:parent.height;color:root.activeFocus?Theme.colors.accent:Theme.hairline}
    MouseArea {id:input;anchors.fill:parent;cursorShape:Qt.SplitHCursor;preventStealing:true
        property real origin:0
        property int initial:0
        property bool gestureCancelled:false
        onPressed:mouse=>{root.forceActiveFocus();origin=mapToItem(null,mouse.x,mouse.y).x;initial=root.value;gestureCancelled=false;}
        onPositionChanged:mouse=>{if(pressed && !gestureCancelled)root.previewRequested(root.bounded(initial+mapToItem(null,mouse.x,mouse.y).x-origin));}
        onReleased:if(gestureCancelled)root.cancelled();else root.committed(root.value)
        onCanceled:root.cancelled()
    }
}

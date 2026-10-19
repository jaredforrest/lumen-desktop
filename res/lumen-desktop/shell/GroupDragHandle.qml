import QtQuick
import QtQuick.Controls
AbstractButton {
    id:root
    required property var controller
    required property var sourceFactory
    property string sourceLabel:"window"
    property var captured:null
    property bool dragging:false
    property bool nativeDragging:false
    property real startX:0
    property real startY:0
    text:"Group…";Accessible.name:"Move "+sourceLabel+" to group";Accessible.role:Accessible.Button
    implicitWidth:76;implicitHeight:22;focusPolicy:Qt.StrongFocus
    function capture(){return typeof sourceFactory==="function"?sourceFactory():null;}
    onPressed:captured=capture()
    onCanceled:captured=null
    onClicked:{const value=captured || capture();captured=null;if(value)controller.begin(value,true);}
    contentItem:PanelLabel {text:root.text;horizontalAlignment:Text.AlignHCenter;verticalAlignment:Text.AlignVCenter;font.pixelSize:10}
    background:Rectangle {radius:6;color:Theme.colors.field;border.color:root.activeFocus?Theme.colors.accent:Theme.hairline}
    Item {id:proxy;width:1;height:1
        Drag.active:root.nativeDragging
        Drag.dragType:Drag.Automatic
        Drag.source:proxy
        Drag.supportedActions:Qt.MoveAction
        Drag.mimeData:({"application/x-lumen-group-intent":root.controller?root.controller.token:""})
        Drag.onDragFinished:{if(root.controller && root.controller.active)root.controller.cancel();Qt.callLater(()=>{root.nativeDragging=false;root.dragging=false;root.captured=null;});}
    }
    MouseArea {anchors.fill:parent;enabled:root.enabled;preventStealing:true
        onPressed:mouse=>{root.captured=root.capture();root.dragging=false;root.startX=mouse.x;root.startY=mouse.y;root.forceActiveFocus();}
        onPositionChanged:mouse=>{
            if(!pressed || !root.captured)return;
            if(!root.dragging && Math.hypot(mouse.x-root.startX,mouse.y-root.startY)>=8)root.dragging=root.controller.begin(root.captured,false);
            if(root.dragging && !root.nativeDragging)root.nativeDragging=true;
        }
        onReleased:mouse=>{const value=root.captured;root.captured=null;if(!root.dragging && value)root.controller.begin(value,true);root.dragging=false;}
        onCanceled:{root.captured=null;if(root.dragging && !root.nativeDragging)root.controller.cancel();if(!root.nativeDragging)root.dragging=false;}
    }
}

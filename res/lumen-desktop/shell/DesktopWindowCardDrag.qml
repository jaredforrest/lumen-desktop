import QtQuick
Item {
    id:root
    required property var controller
    required property var sourceFactory
    property var captured:null
    property bool moved:false
    property bool dragging:false
    property bool nativeDragging:false
    property string intentToken:""
    property real startX:0
    property real startY:0
    readonly property bool pressed:pointer.pressed
    signal pressCaptured()
    signal clicked()
    function capture(){
        try {const value=typeof sourceFactory==="function"?sourceFactory():null;return value?JSON.parse(JSON.stringify(value)):null;}
        catch(error){return null;}
    }
    function retire(){
        captured=null;
        cancelOwn();
        if(!nativeDragging){dragging=false;moved=false;intentToken="";}
    }
    function cancelOwn(){if(dragging && controller && controller.active && controller.token===intentToken)controller.cancel();}
    onEnabledChanged:if(!enabled)retire()
    Component.onDestruction:retire()
    Item {id:proxy;width:1;height:1
        Drag.active:root.nativeDragging
        Drag.dragType:Drag.Automatic
        Drag.source:proxy
        Drag.supportedActions:Qt.MoveAction
        Drag.mimeData:({"application/x-lumen-desktop-window-intent":root.intentToken})
        Drag.onDragFinished:{root.cancelOwn();Qt.callLater(()=>{root.nativeDragging=false;root.dragging=false;root.captured=null;root.moved=false;root.intentToken="";});}
    }
    MouseArea {id:pointer;anchors.fill:parent;acceptedButtons:Qt.LeftButton;preventStealing:true
        onPressed:mouse=>{root.captured=root.capture();root.moved=false;root.dragging=false;root.intentToken="";root.startX=mouse.x;root.startY=mouse.y;root.pressCaptured();}
        onPositionChanged:mouse=>{
            if(!pressed)return;
            if(!root.moved && Math.hypot(mouse.x-root.startX,mouse.y-root.startY)>=8){
                root.moved=true;
                if(root.captured && root.controller && !root.controller.busy)root.dragging=root.controller.begin(root.captured,false);
                if(root.dragging)root.intentToken=root.controller.token;
            }
            if(root.dragging && !root.nativeDragging)root.nativeDragging=true;
        }
        onReleased:mouse=>{const activate=!root.moved && !root.dragging && mouse.x>=0 && mouse.y>=0 && mouse.x<width && mouse.y<height;root.captured=null;if(!root.nativeDragging)root.dragging=false;if(activate)root.clicked();}
        onCanceled:{root.captured=null;if(!root.nativeDragging)root.cancelOwn();if(!root.nativeDragging){root.dragging=false;root.moved=false;root.intentToken="";}}
    }
}

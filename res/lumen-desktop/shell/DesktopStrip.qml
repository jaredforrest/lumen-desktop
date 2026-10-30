import QtQuick
import QtQuick.Controls
import QtQuick.Window
import "DesktopOrderModel.js" as Ordering

Item {
    id:root
    required property var desktops
    required property int revision
    required property string sessionEpoch
    FontMetrics {id:labelMetrics;font.family:Theme.font;font.pixelSize:Theme.textRole("body")}
    implicitHeight:Math.max(30,labelMetrics.height+12)+4
    property bool busy:false
    property var dropController:null
    property string outputId:""
    property string selectedId:""
    property var gesture:null
    property bool pointerMoved:false
    property bool cancelled:false
    property real pressX:0
    readonly property bool editing:gesture!==null
    readonly property var shown:{
        const items=desktops.filter(item=>item.outputId===outputId && item.available);
        return gesture?gesture.order.map(id=>items.find(item=>item.logicalId===id)).filter(Boolean):items;
    }
    signal activateDesktop(var desktop)
    signal reorderRequested(var request)
    signal cancelledEdit(string reason)
    onRevisionChanged:if(gesture && gesture.revision!==revision){cancel();cancelledEdit("Desktop order changed; reorder cancelled");}
    onSessionEpochChanged:if(gesture && gesture.sessionEpoch!==sessionEpoch){cancel();cancelledEdit("Desktop session changed; reorder cancelled");}
    onOutputIdChanged:cancel()
    function begin(id){if(busy || !desktops.some(item=>item.logicalId===id && item.outputId===outputId))return false;gesture=Ordering.begin(desktops,id,revision,sessionEpoch);selectedId=id;cancelled=false;return !!gesture;}
    function cancel(){gesture=null;pointerMoved=false;cancelled=true;}
    function commit(){const request=Ordering.request(gesture,revision,sessionEpoch);gesture=null;pointerMoved=false;if(request)reorderRequested(request);}
    function step(delta){if(!gesture && !begin(selectedId || (shown[0] || {}).logicalId))return;gesture=Ordering.move(gesture,gesture.order.indexOf(gesture.id)+delta);}
    function pointerAt(x){if(!gesture)return;const target=list.indexAt(Math.max(0,Math.min(list.contentWidth-1,x+list.contentX)),12);if(target>=0)gesture=Ordering.move(gesture,target);}
    ListView {id:list;objectName:"desktop-order-strip";anchors.fill:parent;orientation:ListView.Horizontal;spacing:6;clip:true;model:root.shown;interactive:!root.editing
        delegate:DialogButton {
            id:button;required property var modelData
            objectName:"desktop-"+modelData.logicalId
            readonly property real baseWidth:Math.max(100,Math.min(180,text.length*7+24))
            width:Math.max(baseWidth,Math.ceil((baseWidth-28)*Math.max(Theme.textScale,defaultText.implicitWidth>0?scaledText.implicitWidth/defaultText.implicitWidth:1))+28);height:root.implicitHeight-4;
            Text {id:defaultText;visible:false;text:button.text;font.family:Theme.font;font.pixelSize:13}
            Text {id:scaledText;visible:false;text:button.text;font.family:Theme.font;font.pixelSize:Theme.textRole("body")}text:modelData.label || "Desktop "+modelData.name
            primary:modelData.focused;defaultAction:false;enabled:!root.busy
            Accessible.name:"Workspace "+modelData.name+" on "+modelData.output
            Accessible.description:"Drag to reorder. Control plus Left or Right to preview order; Return to save; Escape to cancel."
            Accessible.selected:root.selectedId===modelData.logicalId
            onClicked:{if(root.dropController && root.dropController.active){root.dropController.commit(modelData.transfer);return;}if(!root.editing)root.activateDesktop(modelData);}
            DropArea {anchors.fill:parent
                onEntered:drag=>{if(root.dropController && root.dropController.accepts(drag) && root.dropController.canTarget(button.modelData.transfer))drag.accept(Qt.MoveAction);else drag.accepted=false;}
                onDropped:drop=>{if(root.dropController && root.dropController.accepts(drop) && root.dropController.canTarget(button.modelData.transfer)){root.dropController.commit(button.modelData.transfer);drop.accept(Qt.MoveAction);}else drop.accepted=false;}
                Rectangle {anchors.fill:parent;radius:7;visible:parent.containsDrag;color:"transparent";border.width:2;border.color:Theme.colors.accent}
            }
            Keys.onPressed:event=>{
                if((event.modifiers&Qt.ControlModifier) && [Qt.Key_Left,Qt.Key_Right].includes(event.key)){
                    if(!root.gesture)root.begin(modelData.logicalId);root.step(event.key===Qt.Key_Left?-1:1);event.accepted=true;
                }else if(event.key===Qt.Key_Escape && root.editing){root.cancel();event.accepted=true;}
                else if([Qt.Key_Return,Qt.Key_Enter].includes(event.key) && root.editing){root.commit();event.accepted=true;}
            }

        }
        ScrollBar.horizontal:DesktopScrollBar {}
    }
    MouseArea {id:gesturePointer;objectName:"desktop-order-pointer";anchors.fill:parent;enabled:!root.busy && !(root.dropController && root.dropController.active);preventStealing:root.editing
        onPressed:mouse=>{const index=list.indexAt(mouse.x+list.contentX,12);if(index<0)return;const item=list.itemAtIndex(index);if(item)item.forceActiveFocus();root.pointerMoved=false;root.pressX=mouse.x;root.begin(root.shown[index].logicalId);}
        onPositionChanged:mouse=>{if(!pressed || root.cancelled || !root.gesture)return;if(Math.abs(mouse.x-root.pressX)>8)root.pointerMoved=true;if(root.pointerMoved)root.pointerAt(mouse.x);}
        onReleased:mouse=>{if(root.cancelled || !root.gesture)return;if(mouse.x<0 || mouse.y<0 || mouse.x>width || mouse.y>height){root.cancel();return;}const desktop=root.desktops.find(item=>item.logicalId===root.gesture.id);if(root.pointerMoved)root.commit();else {root.gesture=null;if(desktop)root.activateDesktop(desktop);}}
        onCanceled:root.cancel()
        onWheel:wheel=>{list.contentX=Math.max(0,Math.min(list.contentWidth-list.width,list.contentX-wheel.angleDelta.y));wheel.accepted=true;}
    }
    Shortcut {sequence:"Ctrl+Left";enabled:root.editing && !InputOwnership.composing(root.Window.window);onActivated:root.step(-1)}
    Shortcut {sequence:"Ctrl+Right";enabled:root.editing && !InputOwnership.composing(root.Window.window);onActivated:root.step(1)}
    Shortcut {sequence:"Return";enabled:root.editing && !InputOwnership.composing(root.Window.window);onActivated:root.commit()}
    Shortcut {sequence:"Enter";enabled:root.editing && !InputOwnership.composing(root.Window.window);onActivated:root.commit()}
    Shortcut {sequence:"Escape";enabled:root.editing && !InputOwnership.composing(root.Window.window);onActivated:root.cancel()}

}

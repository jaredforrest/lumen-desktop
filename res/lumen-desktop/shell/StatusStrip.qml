import QtQuick
import "StatusStripModel.js" as StatusModel
import "NetworkState.js" as NetworkState
Row {
    id:root
    property var order:StatusModel.ids
    property var available:({battery:false,sound:true,focus:false,search:true,controls:true})
    property real budget:222
    property var power:null
    property var internet:({status:"unknown",message:"Checking internet connection…"})
    readonly property var internetState:NetworkState.internet(internet)
    property string soundIcon:"sound"
    readonly property var layout:StatusModel.layout(order,available,budget)
    readonly property var labels:({battery:"Battery",sound:"Sound",focus:"Focus",search:"Search",controls:"Controls"})
    signal activated(string id)
    signal reordered(var order)
    spacing:0
    Repeater { id:items
        model:root.layout.visible
        delegate: FocusScope {
            id:cell
            required property string modelData
            objectName:"status-item-"+modelData
            width:StatusModel.itemWidth(modelData);height:24
            activeFocusOnTab:true
            Accessible.role:Accessible.Button
            Accessible.name:root.labels[modelData]+(modelData==="battery" && root.power?": "+Math.round(root.power.percentage || 0)+" percent":modelData==="controls"?": "+root.internetState.message:"")
            Accessible.description:"Hold Super and drag to reorder. Control Shift Left or Right moves this item."
            Accessible.onPressAction:root.activated(modelData)
            Rectangle {anchors.fill:parent;radius:5;color:cell.activeFocus?Theme.colors.hover:"transparent";border.width:cell.activeFocus?1:0;border.color:Theme.colors.accent}
            MenuButton {anchors.fill:parent;enabled:false;Accessible.ignored:true;text:"";iconName:cell.modelData==="sound"?root.soundIcon:cell.modelData==="focus"?"moon":cell.modelData==="battery"?"":cell.modelData;selected:drag.pressed && drag.reordering}
            Rectangle {visible:cell.modelData==="controls" && ["offline","degraded","portal"].includes(root.internetState.status);width:7;height:7;radius:4;color:root.internetState.status==="offline"?Theme.errorText:Theme.warningText;anchors.right:parent.right;anchors.top:parent.top}
            Row {anchors.centerIn:parent;spacing:4;visible:cell.modelData==="battery";BatteryGlyph {anchors.verticalCenter:parent.verticalCenter;percentage:root.power && root.power.percentage || 0;charging:!!root.power && root.power.state==="charging"} PanelLabel {text:Math.round(root.power && root.power.percentage || 0)+"%";font.pixelSize:12}}
            Keys.onEscapePressed:{drag.cancelled=true;drag.reordering=false;}
            Keys.onPressed:event=>{
                if(event.key===Qt.Key_Return || event.key===Qt.Key_Space){root.activated(modelData);event.accepted=true;}
                else if((event.modifiers & Qt.ControlModifier) && (event.modifiers & Qt.ShiftModifier) && [Qt.Key_Left,Qt.Key_Right].includes(event.key)){
                    const owner=root,id=modelData,target=StatusModel.normalize(root.order).indexOf(id)+(event.key===Qt.Key_Left?-1:1);
                    root.reordered(StatusModel.move(root.order,id,target));event.accepted=true;
                    Qt.callLater(()=>owner.focusItem(id));
                }
            }
            MouseArea {
                id:drag;anchors.fill:parent
                property bool reordering:false
                property bool cancelled:false
                property real origin:0
                onPressed:mouse=>{cancelled=false;cell.forceActiveFocus();reordering=!!(mouse.modifiers & Qt.MetaModifier);origin=mapToItem(root,mouse.x,mouse.y).x;}
                onReleased:mouse=>{
                    const owner=root,id=cell.modelData,position=mapToItem(root,mouse.x,mouse.y).x;
                    if(cancelled || mouse.y<0 || mouse.y>height)return;
                    if(reordering){const targetId=root.itemAtPosition(position);const target=StatusModel.normalize(root.order).indexOf(targetId);reordering=false;root.reordered(StatusModel.move(root.order,id,target));Qt.callLater(()=>owner.focusItem(id));}
                    else if(containsMouse && Math.abs(position-origin)<8)root.activated(id);
                }
                onCanceled:reordering=false
            }
        }
    }
    function itemAtPosition(x){let edge=0;for(const id of layout.visible){edge+=StatusModel.itemWidth(id);if(x<edge)return id;}return layout.visible[layout.visible.length-1];}
    function focusItem(id){for(let i=0;i<items.count;i++){const item=items.itemAt(i);if(item.modelData===id){item.forceActiveFocus();return;}}}
}

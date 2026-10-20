import QtQuick
import QtQuick.Controls
Item {
    id:root
    property var tabs:[]
    property string activeId:""
    property string draggedId:""
    property int dropIndex:-1
    property bool gestureCancelled:false
    signal selectRequested(string id)
    signal closeRequested(string id)
    signal moveRequested(string id,int target)
    signal newRequested()
    function revealActive(){row.forceLayout();ensureVisible(activeId);}
    function scheduleReveal(){Qt.callLater(revealActive);}
    function cancelDrag(){gestureCancelled=true;draggedId="";dropIndex=-1;}
    onActiveIdChanged:Qt.callLater(()=>ensureVisible(activeId))
    onTabsChanged:Qt.callLater(()=>ensureVisible(activeId))
    onWidthChanged:Qt.callLater(()=>ensureVisible(activeId))
    function ensureVisible(id){for(let index=0;index<tabItems.count;index++){const item=tabItems.itemAt(index);if(item && item.modelData.id===id){scroll.contentX=Math.max(0,Math.min(Math.max(0,scroll.contentWidth-scroll.width),item.x-scroll.width/2+item.width/2));return;}}}
    function focusTab(id){for(let index=0;index<tabItems.count;index++){const item=tabItems.itemAt(index);if(item && item.modelData.id===id){item.forceActiveFocus();scroll.contentX=Math.max(0,Math.min(scroll.contentWidth-scroll.width,item.x-scroll.width/2+item.width/2));return;}}}
    function requestMove(id,target){moveRequested(id,target);Qt.callLater(()=>focusTab(id));}
    function requestClose(id){closeRequested(id);Qt.callLater(()=>focusTab(activeId));}
    function move(id,delta){const from=tabs.findIndex(tab=>tab.id===id),to=from+delta;if(from>=0 && to>=0 && to<tabs.length)requestMove(id,to);}
    FontMetrics {id:captionMetrics;font.family:Theme.font;font.pixelSize:Theme.textRole("label")}
    readonly property real tabRowHeight:Math.max(28,Math.ceil(captionMetrics.height+12))
    implicitHeight:tabRowHeight+2
    Flickable {id:scroll;anchors {left:parent.left;right:add.left;top:parent.top;bottom:parent.bottom;rightMargin:4}contentWidth:row.width;clip:true;boundsBehavior:Flickable.StopAtBounds;interactive:root.draggedId===""
        Row {id:row;spacing:2;onWidthChanged:root.scheduleReveal()
            Repeater {id:tabItems;model:root.tabs
                Item {id:tab;required property var modelData;required property int index
                    objectName:"document-tab-"+modelData.id
                    readonly property real baseWidth:Math.max(100,Math.min(200,(scroll.width-Math.max(0,root.tabs.length-1)*2)/Math.max(1,root.tabs.length)))
                    readonly property real captionScale:Math.max(Theme.textScale,baselineCaption.implicitWidth>0?caption.implicitWidth/baselineCaption.implicitWidth:1)
                    width:Math.max(baseWidth,Math.ceil((baseWidth-35)*captionScale)+35);height:root.tabRowHeight
                    onWidthChanged:root.scheduleReveal()
                    Text {id:baselineCaption;visible:false;text:caption.text;font.family:Theme.font;font.pixelSize:12}
                    activeFocusOnTab:true
                    Accessible.role:Accessible.PageTab;Accessible.name:modelData.path;Accessible.selected:modelData.id===root.activeId
                    Accessible.description:"Control Shift Left or Right to reorder; Control W to close"
                    Accessible.onPressAction:root.selectRequested(modelData.id)
                    Keys.onPressed:event=>{
                        if(event.key===Qt.Key_Escape){root.cancelDrag();event.accepted=true;}
                        else if((event.modifiers & Qt.ControlModifier) && (event.modifiers & Qt.ShiftModifier) && [Qt.Key_Left,Qt.Key_Right].includes(event.key)){root.move(modelData.id,event.key===Qt.Key_Left?-1:1);event.accepted=true;}
                        else if((event.modifiers & Qt.ControlModifier) && event.key===Qt.Key_W){root.requestClose(modelData.id);event.accepted=true;}
                        else if([Qt.Key_Space,Qt.Key_Return,Qt.Key_Enter].includes(event.key)){root.selectRequested(modelData.id);event.accepted=true;}
                        else event.accepted=false;
                    }
                    Rectangle {anchors.fill:parent;radius:6;color:tab.modelData.id===root.activeId?Theme.colors.surface:input.containsMouse?Theme.colors.hover:Theme.colors.field;border.color:tab.activeFocus?Theme.colors.accent:Theme.hairline;opacity:root.draggedId===tab.modelData.id ? .65 : 1}
                    PanelLabel {id:caption;x:8;width:parent.width-35;height:parent.height;text:tab.modelData.path==="trash:"?"Trash":tab.modelData.path.split("/").filter(Boolean).pop() || "Computer";elide:Text.ElideRight;verticalAlignment:Text.AlignVCenter;font.pixelSize:Theme.textRole("label")}
                    MouseArea {id:input;anchors.fill:parent;hoverEnabled:true;preventStealing:true
                        property real pressX:0
                        property real pressY:0
                        property bool moved:false
                        onPressed:mouse=>{tab.forceActiveFocus();pressX=mapToItem(root,mouse.x,mouse.y).x;pressY=mapToItem(root,mouse.x,mouse.y).y;moved=false;root.gestureCancelled=false;}
                        onPositionChanged:mouse=>{if(!pressed || root.gestureCancelled)return;const at=mapToItem(root,mouse.x,mouse.y),x=at.x;if(Math.abs(x-pressX)>8 || Math.abs(at.y-pressY)>8)moved=true;if(moved){root.draggedId=tab.modelData.id;const local=mapToItem(row,mouse.x,mouse.y);root.dropIndex=Math.max(0,Math.min(root.tabs.length-1,Math.floor(local.x/(tab.width+row.spacing))));if(x<18)scroll.contentX=Math.max(0,scroll.contentX-12);else if(x>scroll.width-18)scroll.contentX=Math.min(Math.max(0,scroll.contentWidth-scroll.width),scroll.contentX+12);}}
                        onReleased:mouse=>{const owner=root,id=tab.modelData.id,target=owner.dropIndex,at=mapToItem(owner,mouse.x,mouse.y);const move=moved && !owner.gestureCancelled && owner.draggedId===id && at.x>=0 && at.x<=scroll.width && mouse.y>=-12 && mouse.y<=height+12 && target>=0;const select=!moved && !owner.gestureCancelled && mouse.x>=0 && mouse.x<=width && mouse.y>=0 && mouse.y<=height;owner.cancelDrag();if(move)owner.requestMove(id,target);else if(select)owner.selectRequested(id);}
                        onCanceled:root.cancelDrag()
                    }
                    AbstractButton {objectName:"document-tab-close-"+tab.modelData.id;anchors.right:parent.right;anchors.verticalCenter:parent.verticalCenter;width:25;height:25;Accessible.name:"Close "+tab.modelData.path;onClicked:root.requestClose(tab.modelData.id)
                        contentItem:PanelLabel {text:"×";horizontalAlignment:Text.AlignHCenter;verticalAlignment:Text.AlignVCenter}
                        background:Rectangle {radius:5;color:parent.hovered || parent.activeFocus?Theme.colors.hover:"transparent"}
                    }
                    Rectangle {x:0;y:0;width:2;height:parent.height;visible:root.draggedId!=="" && root.dropIndex===tab.index;color:Theme.colors.accent}
                }
            }
        }
    }
    ControlRow {id:add;anchors.right:parent.right;width:28;height:28;text:"+";Accessible.name:"New tab";onClicked:root.newRequested()}
}

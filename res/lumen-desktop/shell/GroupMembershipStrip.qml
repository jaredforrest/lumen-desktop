import QtQuick
import QtQuick.Controls
FocusScope {
    id:root
    required property var controller
    visible:!!controller && controller.active
    height:58
    Connections {target:root.controller || null
        function onBegan(){if(root.controller.keyboardMode)Qt.callLater(()=>root.forceActiveFocus());}
        function onSelectedChanged(){const item=targets.itemAt(root.controller.selected);if(item){if(item.x<scroll.contentX)scroll.contentX=item.x;else if(item.x+item.width>scroll.contentX+scroll.width)scroll.contentX=item.x+item.width-scroll.width;}}
    }
    Rectangle {anchors.fill:parent;radius:10;color:Theme.colors.background;border.color:Theme.colors.accent}
    PanelLabel {x:10;y:5;width:parent.width-100;text:"Move window to group · release to apply · Escape to cancel";font.pixelSize:10;color:Theme.colors.muted}
    Flickable {id:scroll;x:8;y:22;width:parent.width-94;height:30;contentWidth:row.width;clip:true;boundsBehavior:Flickable.StopAtBounds
        Row {id:row;spacing:6
            Repeater {id:targets;model:root.controller?root.controller.destinations:[]
                delegate:DialogButton {required property var modelData;required property int index;objectName:modelData.kind==="new"?"group-drop-new":"group-drop-"+modelData.groupId;width:104;height:28;text:modelData.label
                    defaultAction:false;primary:(root.controller.keyboardMode && root.controller.selected===index) || (!!root.controller.target && root.controller.target.kind===modelData.kind && (modelData.kind==="new" || root.controller.target.groupId===modelData.groupId))
                    onClicked:root.controller.commit(Object.assign({},modelData))
                    DropArea {anchors.fill:parent;keys:["application/x-lumen-group-intent"]
                        onEntered:drag=>{if(!root.controller.accepts(drag)){drag.accepted=false;return;}root.controller.preview(modelData);drag.accept(Qt.MoveAction);}
                        onExited:root.controller.target=null
                        onDropped:drop=>{if(root.controller.accepts(drop)){root.controller.commit(Object.assign({},modelData));drop.accept(Qt.MoveAction);}else drop.accepted=false;}
                    }
                }
            }
        }
    }
    DialogButton {x:parent.width-80;y:22;width:72;height:28;text:"Cancel";onClicked:root.controller.cancel()}
    Keys.onPressed:event=>{
        if(event.key===Qt.Key_Escape){controller.cancel();event.accepted=true;}
        else if([Qt.Key_Left,Qt.Key_Right].includes(event.key)){controller.step(event.key===Qt.Key_Left?-1:1);event.accepted=true;}
        else if([Qt.Key_Return,Qt.Key_Enter,Qt.Key_Space].includes(event.key)){controller.commit(controller.destinations[controller.selected]);event.accepted=true;}
    }
}

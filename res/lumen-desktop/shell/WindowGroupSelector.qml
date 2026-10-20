import QtQuick
import QtQuick.Controls
import Quickshell
import Quickshell.Io
import Quickshell.Wayland
PanelWindow {
    id:root
    required property var shell
    required property var service
    required property string outputName
    readonly property var nativeState:service && service.state?service.state:({available:false,enabled:false,locked:true,outputs:[]})
    readonly property var output:(nativeState.outputs || []).find(o=>o.name===outputName) || null
    readonly property bool available:nativeState.available===true && nativeState.enabled===true && nativeState.locked===false && !!output && !output.suspended && output.groups.length>0
    visible:available
    anchors {left:true;top:true;bottom:true}
    implicitWidth:keyboardMode && screen?screen.width:202;exclusiveZone:0;exclusionMode:ExclusionMode.Ignore;color:"transparent"
    WlrLayershell.namespace:"lumen-window-groups"
    WlrLayershell.layer:WlrLayer.Overlay
    WlrLayershell.keyboardFocus:keyboardMode && revealed?WlrKeyboardFocus.Exclusive:WlrKeyboardFocus.None
    readonly property var membership:shell && shell.groupMembershipDrag?shell.groupMembershipDrag:null
    readonly property bool membershipMode:!!membership && membership.active && !!membership.source && membership.source.output===outputName && !!output && membership.source.workspace===output.workspace
    property bool membershipShown:false
    Connections {target:root.membership;function onActiveChanged(){if(!root.membership.active && root.membershipShown)root.dismiss();}}
    property bool revealed:false
    property bool keyboardMode:false
    property var rows:[]
    property var capture:null
    property var pressed:null
    property int selected:0
    property string error:""
    property int commits:0
    function dismiss(){membershipShown=false;revealTimer.stop();hideTimer.stop();revealed=false;keyboardMode=false;pressed=null;capture=null;rows=[];if(shell.popupOwner==="groups:"+outputName)shell.popupOwner="";}
    function begin(keyboard){
        if(!available || worker.running || (!keyboard && shell.popupOwner!=="" && shell.popupOwner!=="groups:"+outputName))return;
        hideTimer.stop();revealTimer.stop();
        if(!revealed){rows=output.groups.map(g=>Object.assign({},g));capture={epoch:nativeState.epoch,revision:nativeState.revision,workspace:output.workspace,output:outputName,focusedOutput:nativeState.focusedOutput};selected=Math.max(0,rows.findIndex(g=>g.selected));revealed=true;error="";}
        if(keyboard){shell.claimPopup("groups:"+outputName);keyboardMode=true;Qt.callLater(()=>keys.forceActiveFocus());}
    }
    function target(index){return capture && rows[index]?Object.assign({id:rows[index].id},capture):null;}
    function commit(value){
        if(!value || !available || !output || !capture || value.epoch!==nativeState.epoch || value.revision!==nativeState.revision || value.workspace!==output.workspace){dismiss();return;}
        const epoch=shell.focusEpoch;dismiss();pending.data=value;pending.focusEpoch=epoch;pending.restart();
    }
    function scopeChanged(){if(revealed && (!available || !output || !capture || capture.epoch!==nativeState.epoch || capture.revision!==nativeState.revision || capture.workspace!==output.workspace || (keyboardMode && !pressed && nativeState.focusedOutput!==capture.focusedOutput)))dismiss();}
    onNativeStateChanged:scopeChanged()
    onAvailableChanged:if(!available)dismiss()
    onOutputChanged:scopeChanged()
    Connections {target:shell
        function onRequestGroupSelector(name){if(name===root.outputName)root.begin(true);else root.dismiss();}
        function onDismissPopups(owner){if(owner!=="groups:"+root.outputName)root.dismiss();}
    }
    Component.onCompleted:{const value=Object.assign({},shell.groupSurfaces);value[outputName]=root;shell.groupSurfaces=value;}
    Component.onDestruction:{const value=Object.assign({},shell.groupSurfaces);delete value[outputName];shell.groupSurfaces=value;}
    Timer {id:revealTimer;interval:180;onTriggered:root.begin(false)}
    Timer {id:hideTimer;interval:350;onTriggered:if(!root.membershipMode && !root.keyboardMode && !edge.containsMouse && !cardHover.containsMouse)root.dismiss()}
    Timer {id:pending;interval:0;property var data:null;property int focusEpoch:0;onTriggered:if(focusEpoch===root.shell.focusEpoch){root.request=data;worker.running=true;}}
    property var request:null
    Process {id:worker;stdinEnabled:true
        command:[Quickshell.env("GLOBAL_MENU_PYTHON") || "python3",Qt.resolvedUrl("window_group_selector.py").toString().replace(/^file:\/\//,"")]
        onStarted:write(JSON.stringify(root.request)+"\n")
        stdout:StdioCollector {onStreamFinished:{try{const result=JSON.parse(text);root.error=result.error || "";if(result.ok)root.commits++;}catch(error){root.error="Unable to select the captured group";}}}
    }
    mask:Region {
        Region {width:root.keyboardMode?root.width:0;height:root.keyboardMode?root.height:0}
        Region {x:0;y:80;width:6;height:Math.max(0,root.height-160)}
        Region {x:8;y:card.y;width:root.revealed?card.width:0;height:root.revealed?card.height:0}
    }
    MouseArea {anchors.fill:parent;visible:root.keyboardMode;onClicked:{if(root.membershipMode)root.membership.cancel();root.dismiss();}}
    MouseArea {id:edge;z:2;objectName:"window-group-edge";x:0;y:80;width:6;height:Math.max(0,root.height-160);hoverEnabled:true
        onEntered:{hideTimer.stop();revealTimer.restart();}
        onExited:{revealTimer.stop();hideTimer.restart();}
        onClicked:root.begin(true)
        DropArea {anchors.fill:parent;keys:["application/x-lumen-group-intent"]
            onEntered:drag=>{if(!root.membershipMode || !root.membership.accepts(drag)){drag.accepted=false;return;}root.begin(false);root.membershipShown=root.revealed;drag.accept(Qt.MoveAction);}
        }
    }
    Rectangle {id:card;z:1;objectName:"window-group-card";x:8;y:(root.height-height)/2;width:186;height:Math.min(Math.max(110,root.rows.length*58+58+(root.membershipMode?38:0)),Math.max(110,root.height-160));visible:root.revealed;radius:14;color:Theme.colors.background;border.color:Theme.outline
        MouseArea {id:cardHover;anchors.fill:parent;hoverEnabled:true;acceptedButtons:Qt.NoButton;onEntered:hideTimer.stop();onExited:hideTimer.restart()}
        PanelLabel {x:12;y:12;width:parent.width-24;text:"Recent Groups";font.weight:Font.DemiBold;font.pixelSize:12}
        ListView {id:list;x:8;y:38;width:parent.width-16;height:parent.height-46-(root.membershipMode?38:0);clip:true;spacing:4;model:root.rows;currentIndex:root.selected
            ScrollBar.vertical:DesktopScrollBar {}
            delegate:Rectangle {
                id:row;required property var modelData;required property int index;objectName:"window-group-row-"+modelData.id;width:list.width;height:54;radius:9;opacity:root.membershipMode && modelData.id===root.membership.source.sourceGroup?0.45:1;color:((root.membershipMode && root.membership.target && root.membership.target.groupId===modelData.id) || (!root.membershipMode && root.selected===index))?Theme.colors.accent:Theme.colors.surface
                PanelLabel {x:10;y:8;width:parent.width-20;text:"Group "+row.modelData.id;textFormat:Text.PlainText;font.pixelSize:12;color:root.selected===row.index?Theme.colors.selectedText:Theme.colors.text}
                PanelLabel {x:10;y:29;width:parent.width-20;text:row.modelData.count+(row.modelData.count===1?" window":" windows")+(row.modelData.selected?" · selected":"");font.pixelSize:10;color:root.selected===row.index?Theme.colors.selectedText:Theme.colors.muted}
                DropArea {anchors.fill:parent;keys:["application/x-lumen-group-intent"]
                    onEntered:drag=>{if(!root.membershipMode || row.modelData.id===root.membership.source.sourceGroup || !root.membership.accepts(drag)){drag.accepted=false;return;}root.membership.preview({kind:"group",groupId:row.modelData.id});drag.accept(Qt.MoveAction);}
                    onExited:if(root.membership)root.membership.target=null
                    onDropped:drop=>{if(root.membershipMode && root.membership.accepts(drop)){root.membership.commit({kind:"group",groupId:row.modelData.id});drop.accept(Qt.MoveAction);}else drop.accepted=false;}
                }
                MouseArea {anchors.fill:parent;hoverEnabled:true;Accessible.name:"Select window group "+row.modelData.id;Accessible.role:Accessible.Button
                    onEntered:if(!root.pressed)root.selected=row.index
                    onPressed:{root.begin(true);root.selected=row.index;root.pressed=root.target(row.index);}
                    onCanceled:root.pressed=null
                    onReleased:mouse=>{const value=root.pressed;root.pressed=null;if(containsMouse){if(root.membershipMode)root.membership.commit({kind:"group",groupId:row.modelData.id});else root.commit(value);}else root.dismiss();}
                }
            }
        }
    }
    DialogButton {id:newGroup;objectName:"selector-group-drop-new";x:16;y:card.y+card.height-36;width:170;height:28;z:3;visible:root.revealed && root.membershipMode;text:"New Group";onClicked:root.membership.commit({kind:"new"})
        DropArea {anchors.fill:parent;keys:["application/x-lumen-group-intent"]
            onEntered:drag=>{if(!root.membershipMode || !root.membership.accepts(drag)){drag.accepted=false;return;}root.membership.preview({kind:"new"});drag.accept(Qt.MoveAction);}
            onExited:if(root.membership)root.membership.target=null
            onDropped:drop=>{if(root.membershipMode && root.membership.accepts(drop)){root.membership.commit({kind:"new"});drop.accept(Qt.MoveAction);}else drop.accepted=false;}
        }
    }
    Item {id:keys;focus:root.keyboardMode
        Keys.onPressed:event=>{
            if(event.key===Qt.Key_Escape){if(root.membershipMode)root.membership.cancel();root.dismiss();event.accepted=true;}
            else if([Qt.Key_Up,Qt.Key_Down,Qt.Key_Home,Qt.Key_End].includes(event.key)){
                const n=root.rows.length;if(n){root.selected=event.key===Qt.Key_Home?0:event.key===Qt.Key_End?n-1:(root.selected+(event.key===Qt.Key_Down?1:-1)+n)%n;list.positionViewAtIndex(root.selected,ListView.Contain);}event.accepted=true;
            }else if([Qt.Key_Return,Qt.Key_Enter,Qt.Key_Space].includes(event.key)){root.commit(root.target(root.selected));event.accepted=true;}
        }
    }
}

import QtQuick
import QtQuick.Controls
import QtQuick.Window
import Quickshell
import Quickshell.Io
FloatingWindow {
    id:root
    required property var shell
    title:"Window Overview";visible:false;color:"transparent"
    implicitWidth:880;implicitHeight:560;minimumSize:Qt.size(560,480)
    FontMetrics {id:titleMetrics;font.family:Theme.font;font.pixelSize:Theme.textRole("label")}
    FontMetrics {id:metadataMetrics;font.family:Theme.font;font.pixelSize:Theme.textRole("metadata")}
    FontMetrics {id:captionMetrics;font.family:Theme.font;font.pixelSize:Theme.textRole("caption")}
    readonly property real cardHeight:Math.max(100,13+Math.max(38,Math.ceil(titleMetrics.height*2)+8)+10+Math.ceil(metadataMetrics.height)+Math.max(22,Math.ceil(captionMetrics.height)+8)+3)
    property var windows:[]
    property var workspaces:[]
    property var outputs:[]
    property var windowGroups:null
    readonly property var currentWindowGroup:windowGroups && current?windowGroups.groups.find(g=>g.members.some(m=>m.id===current.id && m.pid===current.pid)) || null:null
    function captureGroupAction(action,groupId){if(!windowGroups)return null;const data={action:"window-groups",groupAction:action,groupId:groupId,groupRevision:windowGroups.revision,sessionEpoch:sessionEpoch};if(current){data.id=current.id;data.pid=current.pid;}return data;}
    function groupAction(action,groupId){const data=captureGroupAction(action,groupId);if(data)send(data);}
    property bool autoReorder:true
    property bool removalSupported:false
    property int revision:0
    property string sessionEpoch:""
    property string managedDesktopId:""
    onManagedDesktopIdChanged:desktopStrip.selectedId=managedDesktopId
    property var removalCapture:null
    readonly property var managedDesktop:workspaces.find(item=>item.logicalId===managedDesktopId) || null
    readonly property var removalDestinations:removalCapture?workspaces.filter(item=>item.available && item.outputId===removalCapture.source.outputId && item.logicalId!==removalCapture.source.logicalId):[]
    function captureGroupWindow(window){
        const drag=shell.groupMembershipDrag;
        if(!drag || drag.state.membershipSupported!==true || !window || window.groupSource!==true || !windowGroups || windowGroups.revision!==drag.state.revision || sessionEpoch!==drag.state.epoch)return null;
        const group=windowGroups.groups.find(g=>g.members.some(m=>m.id===window.id && m.pid===window.pid));
        const output=group?(drag.state.outputs || []).find(o=>o.workspace===group.workspace && !o.suspended):null;
        if(!group || !output)return null;
        return {id:window.id,pid:window.pid,sourceGroup:group.id,workspace:group.workspace,output:output.name,epoch:sessionEpoch,revision:windowGroups.revision};
    }
    Connections {target:root;function onVisibleChanged(){if(!root.visible && root.shell.groupMembershipDrag)root.shell.groupMembershipDrag.cancel();}}
    Connections {target:root.shell.groupMembershipDrag || null;function onCommitsChanged(){if(root.visible)root.send({action:"status"});}function onErrorChanged(){if(root.shell.groupMembershipDrag.error)root.error=root.shell.groupMembershipDrag.error;}}
    function beginRemoval(){if(!managedDesktop || busy)return;removalCapture={source:Object.assign({},managedDesktop),revision:revision,sessionEpoch:sessionEpoch};removeDestination.currentIndex=0;removeDialog.open();}

    property var assignments:({})
    readonly property var assignmentChoices:{
        const choices=[{label:"No assignment",kind:"none",target:null},{label:"All desktops on current display",kind:"all",target:null}].concat(workspaces.map(w=>({label:"Always on "+(w.label || "Desktop "+w.name),kind:"desktop",target:w.logicalId})),outputs.map(o=>({label:"Always on "+o.label,kind:"display",target:o.logicalId})));
        const saved=current?assignments[current.application]:null;
        if(saved && !choices.some(item=>item.kind===saved.kind && item.target===saved.target_id))choices.push({label:"Saved assignment · target unavailable",kind:saved.kind,target:saved.target_id});
        return choices;
    }
    readonly property bool overviewActive:content.Window.window?content.Window.window.active:false
    onOverviewActiveChanged:if(!overviewActive)desktopStrip.cancel()
    property string appFilter:""
    property string error:""
    property int selected:0
    property double draggedId:0
    onVisibleChanged:{if(visible)fit();else {draggedId=0;activation.stop();desktopStrip.cancel();removeDialog.close();groupsDialog.close();}}
    function syncAssignment(){const saved=current?assignments[current.application]:null;assignment.currentIndex=saved?Math.max(0,assignmentChoices.findIndex(item=>item.kind===saved.kind && item.target===saved.target_id)):0;}
    onCurrentChanged:syncAssignment()
    onAssignmentsChanged:syncAssignment()
    function fit(){if(screen){implicitWidth=Math.max(400,Math.min(880,screen.width-32));implicitHeight=Math.max(480,Math.min(560,screen.height-130));}}
    readonly property bool busy:worker.running
    property var request:({})
    readonly property var matches:windows.filter(w=>(!appFilter || w.group===appFilter) && (w.title+" "+w.app+" "+w.workspace).toLowerCase().includes(search.text.toLowerCase()))
    readonly property var current:matches[selected] || null
    function show(app){activation.stop();draggedId=0;if(visible){visible=false;return;}appFilter=app || "";search.text="";selected=0;shell.claimPopup("");visible=true;send({action:"status"});Qt.callLater(()=>{if(content.Window.window)content.Window.window.requestActivate();search.forceActiveFocus();});}
    function showForWindow(id){
        if(!Number.isSafeInteger(id) || id<=0)return;
        activation.stop();
        appFilter="";search.text="";draggedId=id;selected=-1;shell.claimPopup("");visible=true;
        send({action:"status"});
        Qt.callLater(()=>{if(content.Window.window)content.Window.window.requestActivate();});
    }
    function send(data){if(worker.running)return;request=data;error="";worker.running=true;}
    function activate(){if(!busy && current){visible=false;activation.data={action:"focus",id:current.id,pid:current.pid};activation.restart();}}
    function step(delta){draggedId=0;selected=Math.max(0,Math.min(matches.length-1,selected+delta));grid.positionViewAtIndex(selected,GridView.Contain);}
    Timer {id:activation;property var data;interval:100;onTriggered:{if(root.busy)restart();else root.send(data);}}
    Rectangle {id:content;anchors.fill:parent;radius:Theme.nativeWindowFrame?0:12;color:Theme.colors.background;border.color:Theme.outline;focus:true
        Keys.onEscapePressed:{if(root.shell.groupMembershipDrag && root.shell.groupMembershipDrag.active)root.shell.groupMembershipDrag.cancel();else if(desktopStrip.editing)desktopStrip.cancel();else root.visible=false;}
        Keys.onPressed:event=>{
            if(event.key===Qt.Key_Down){root.step(Math.max(1,Math.floor(grid.width/grid.cellWidth)));event.accepted=true;}
            else if(event.key===Qt.Key_Up){root.step(-Math.max(1,Math.floor(grid.width/grid.cellWidth)));event.accepted=true;}
            else if(event.key===Qt.Key_Left){root.step(-1);event.accepted=true;}
            else if(event.key===Qt.Key_Right){root.step(1);event.accepted=true;}
            else if(event.key===Qt.Key_Return || event.key===Qt.Key_Enter){root.activate();event.accepted=true;}
        }
        MouseArea {width:parent.width;height:44;onPressed:if(content.Window.window)content.Window.window.startSystemMove()}
        WindowControls {windowName:"Window Overview";canMinimize:false;canFullscreen:false;windowActive:content.Window.window?content.Window.window.active:false;onTriggered:index=>{if(index===0)root.visible=false;}}
        PanelLabel {x:100;y:14;visible:!Theme.nativeWindowFrame;text:root.appFilter?"Application Windows":"Windows & Workspaces";font.weight:Font.DemiBold}
        FocusFlickable {id:bodyViewport;objectName:"overview-body-scroll";x:0;y:44;width:parent.width;height:parent.height-y;clip:true;contentWidth:width;contentHeight:body.height
            ScrollBar.vertical:DesktopScrollBar {}
            Item {id:body;width:bodyViewport.width;height:Math.max(bodyViewport.height,grid.y+root.cardHeight+footer.implicitHeight+29)
        TextField {id:search;x:20;y:10;width:parent.width-40;height:Math.max(32,contentItem.implicitHeight+12);placeholderText:"Find a window";placeholderTextColor:Theme.colors.muted;color:Theme.colors.text;font.family:Theme.font;font.pixelSize:Theme.textRole("label");selectByMouse:true;Accessible.name:"Find a window";onTextChanged:{root.draggedId=0;root.selected=0;}onAccepted:root.activate();background:Rectangle {radius:8;color:Theme.colors.field;border.color:search.activeFocus?Theme.colors.accent:Theme.hairline}}
        DesktopStrip {id:desktopStrip;x:20;y:search.y+search.height+12;width:parent.width-40;height:implicitHeight;desktops:root.workspaces;revision:root.revision;sessionEpoch:root.sessionEpoch;busy:root.busy;outputId:root.outputs[desktopOutput.currentIndex]?root.outputs[desktopOutput.currentIndex].logicalId:"";selectedId:root.managedDesktopId
            onSelectedIdChanged:root.managedDesktopId=selectedId
            onActivateDesktop:desktop=>{root.managedDesktopId=desktop.logicalId;root.visible=false;activation.data={action:"workspace",logicalId:desktop.logicalId,sessionEpoch:root.sessionEpoch};activation.restart();}
            onReorderRequested:request=>root.send(request)
            onCancelledEdit:reason=>root.error=reason
        }
        Flow {id:desktopCommands;x:20;y:desktopStrip.y+desktopStrip.height+8;spacing:6;width:parent.width-40
            DesktopComboBox {id:desktopOutput;width:Math.min(parent.width,120*Theme.textScale);model:root.outputs;textRole:"label";Accessible.name:"Desktop display";enabled:!root.busy && !desktopStrip.editing;onCurrentIndexChanged:{const id=(root.outputs[currentIndex] || {}).logicalId;if(!root.managedDesktop || root.managedDesktop.outputId!==id){const next=root.workspaces.find(item=>item.available && item.outputId===id);root.managedDesktopId=next?next.logicalId:"";}}}
            DesktopComboBox {id:manageDesktop;width:Math.min(parent.width,150*Theme.textScale);model:root.workspaces.filter(item=>item.outputId===(root.outputs[desktopOutput.currentIndex] || {}).logicalId && item.available);textRole:"label";Accessible.name:"Manage desktop";enabled:!root.busy && !desktopStrip.editing;onActivated:index=>root.managedDesktopId=model[index].logicalId}
            DialogButton {width:Math.max(76,implicitWidth);text:"New";Accessible.name:"Create desktop";enabled:!root.busy && !desktopStrip.editing && root.outputs.length>0;onClicked:root.send({action:"desktop-create",outputId:root.outputs[desktopOutput.currentIndex].logicalId,revision:root.revision,sessionEpoch:root.sessionEpoch})}
            DialogButton {width:Math.max(76,implicitWidth);text:"Remove";Accessible.name:"Remove desktop";enabled:root.removalSupported && !root.busy && !desktopStrip.editing && !!root.managedDesktop && root.workspaces.filter(item=>item.outputId===root.managedDesktop.outputId && item.available).length>1;onClicked:root.beginRemoval()}
        }
        Flow {id:orderCommands;x:20;y:desktopCommands.y+desktopCommands.implicitHeight+6;spacing:6;width:parent.width-manageGroups.width-50
            DialogButton {width:42;text:"←";Accessible.name:"Move desktop left";enabled:!root.busy;onClicked:{desktopStrip.selectedId=root.managedDesktopId;desktopStrip.step(-1);}}
            DialogButton {width:42;text:"→";Accessible.name:"Move desktop right";enabled:!root.busy;onClicked:{desktopStrip.selectedId=root.managedDesktopId;desktopStrip.step(1);}}
            DialogButton {width:Math.max(80,implicitWidth);text:"Save Order";visible:desktopStrip.editing;enabled:!root.busy;onClicked:desktopStrip.commit()}
            DialogButton {width:Math.max(65,implicitWidth);text:"Cancel";visible:desktopStrip.editing;onClicked:desktopStrip.cancel()}
        }
        DialogButton {id:manageGroups;x:parent.width-width-20;y:orderCommands.y;width:Math.max(128,implicitWidth);objectName:"window-groups-open";text:"Window Groups";Accessible.name:"Manage window groups";enabled:!!root.windowGroups && !desktopStrip.editing;onClicked:groupsDialog.open()}

        PanelLabel {id:orderHelp;x:20;y:orderCommands.y+Math.max(30,orderCommands.implicitHeight,manageGroups.height)+5;width:parent.width-40;height:Math.max(16,implicitHeight);wrapMode:Text.WordWrap;font.pixelSize:Theme.textRole("caption");color:Theme.colors.muted;text:desktopStrip.editing?"Manual order turns off recent-use ordering · Return to save · Escape to cancel":"Drag desktops to reorder · Control + Left/Right to preview order"}

        GridView {id:grid;objectName:"overview-window-grid";x:20;y:orderHelp.y+orderHelp.height+9;width:parent.width-40;height:parent.height-y-footer.implicitHeight-29;clip:true;cellWidth:Math.floor(width/Math.max(1,Math.floor(width/(250*Theme.textScale))));cellHeight:root.cardHeight+8;model:root.matches;ScrollBar.vertical:DesktopScrollBar {}
            delegate:AbstractButton {
                id:windowCard
                required property var modelData;required property int index
                width:grid.cellWidth-8;height:root.cardHeight;enabled:!root.busy
                Accessible.name:modelData.title+" · "+modelData.app+" · workspace "+modelData.workspace;Accessible.selected:index===root.selected
                onClicked:{root.draggedId=0;root.selected=index;root.activate();}
                background:Rectangle {radius:12;color:Theme.colors.surface;border.width:parent.index===root.selected?2:1;border.color:parent.index===root.selected?Theme.colors.accent:Theme.hairline}
                contentItem:Item {
                    SystemGlyph {x:12;y:15;width:24;height:24;name:"screen";ink:Theme.colors.accent}
                    PanelLabel {id:cardTitle;objectName:"overview-card-title";x:46;y:13;width:parent.width-58;height:Math.max(38,Math.ceil(titleMetrics.height*2)+8);text:modelData.title;textFormat:Text.PlainText;wrapMode:Text.Wrap;maximumLineCount:2;font.weight:Font.DemiBold;font.pixelSize:Theme.textRole("label")}
                    GroupDragHandle {sourceLabel:windowCard.modelData.title;objectName:"group-drag-"+windowCard.modelData.id;x:parent.width-88;y:parent.height-height-2;controller:root.shell.groupMembershipDrag;sourceFactory:()=>root.captureGroupWindow(windowCard.modelData);enabled:!!root.captureGroupWindow(windowCard.modelData)}
                    PanelLabel {id:cardDesktop;objectName:"overview-card-desktop";x:12;y:cardTitle.y+cardTitle.height+10;width:parent.width-24;text:modelData.workspace==="__i3_scratch"?"Minimised":modelData.workspaceLabel || "Desktop "+modelData.workspace;textFormat:Text.PlainText;color:Theme.colors.muted;font.pixelSize:Theme.textRole("metadata")}
                }
            }
            PanelLabel {visible:root.matches.length===0;anchors.centerIn:parent;text:"No matching windows";color:Theme.colors.muted}
        }
        GroupMembershipStrip {x:20;y:footer.y+10;width:parent.width-40;z:30;controller:root.shell.groupMembershipDrag}
        Column {id:footer;x:20;y:grid.y+grid.height+17;width:parent.width-40;spacing:10;visible:!(root.shell.groupMembershipDrag && root.shell.groupMembershipDrag.active)
        Row {width:parent.width;height:Math.max(30,workspace.implicitHeight,moveCommand.implicitHeight,output.implicitHeight);spacing:8
            DesktopComboBox {id:workspace;width:(parent.width-136)/2;model:root.workspaces.filter(item=>item.available);textRole:"label";Accessible.name:"Destination desktop"}
            DialogButton {id:moveCommand;width:120;text:"Move Window";enabled:!!root.current && !root.busy && workspace.count>0;onClicked:root.send({action:"move",id:root.current.id,pid:root.current.pid,logicalId:workspace.model[workspace.currentIndex].logicalId,sessionEpoch:root.sessionEpoch})}
            DesktopComboBox {id:output;width:(parent.width-136)/2;enabled:!!root.current && !root.busy;model:root.outputs;textRole:"label";Accessible.name:"Destination display";onActivated:index=>{if(root.current)root.send({action:"output",id:root.current.id,pid:root.current.pid,output:root.outputs[index].name});}}
        }
        Row {spacing:8;width:parent.width;height:Math.max(30,assignment.implicitHeight,assignCommand.implicitHeight)
            DesktopComboBox {id:assignment;width:parent.width-168;model:root.assignmentChoices;textRole:"label";Accessible.name:"Persistent application desktop assignment"}
            DialogButton {id:assignCommand;width:160;text:"Assign Application";enabled:!!root.current && !!root.current.application && !root.busy;onClicked:{const choice=root.assignmentChoices[assignment.currentIndex];root.send({action:"assign",id:root.current.id,pid:root.current.pid,kind:choice.kind,target:choice.target,sessionEpoch:root.sessionEpoch});}}
        }
        Flow {width:parent.width;spacing:20
        PanelLabel {width:Math.max(200,parent.width-recentOrdering.width-20);topPadding:4;font.pixelSize:Theme.textRole("metadata");color:Theme.colors.muted;text:root.error || (root.current && root.current.allDesktopMembership && root.current.allDesktopMembership.reason ? root.current.allDesktopMembership.reason : "") || (root.draggedId>0?"Choose a desktop, then Move Window · Escape to cancel":"Arrow keys to select · Return to open · Escape to close");wrapMode:Text.WordWrap}
        SettingsToggle {id:recentOrdering;width:Math.min(parent.width,250*Theme.textScale);height:Math.max(36,implicitHeight);text:"Recent desktop order";enabled:!root.busy;Accessible.name:"Order desktops by recent use";checked:root.autoReorder;onToggled:root.send({action:"ordering",enabled:checked})}

        }

        }
            }
        }
        Dialog {id:groupsDialog;focus:true;onOpened:groupChoice.forceActiveFocus();onClosed:if(root.visible)manageGroups.forceActiveFocus();implicitWidth:480;title:"Window Groups";header:PanelLabel {text:"Window Groups";padding:12;font.weight:Font.DemiBold;color:Theme.colors.text}modal:true;anchors.centerIn:parent;width:Math.min(480,parent.width-32);closePolicy:Popup.CloseOnEscape
            background:Rectangle {radius:12;color:Theme.colors.background;border.color:Theme.outline}
            contentItem:Column {spacing:10
                SettingsToggle {objectName:"window-groups-toggle";property var captured:null;width:parent.width;text:"Window Groups";onPressed:captured=root.captureGroupAction(checked?"off":"on",0);checked:!!root.windowGroups && root.windowGroups.enabled;enabled:!root.busy && !!root.windowGroups && !root.windowGroups.locked && (root.windowGroups.separate_displays || root.windowGroups.enabled);onCanceled:captured=null;onToggled:{const intent=captured || root.captureGroupAction(checked?"on":"off",0);captured=null;if(intent)root.send(intent);}}
                PanelLabel {width:parent.width;wrapMode:Text.WordWrap;font.pixelSize:Theme.textRole("metadata");color:Theme.colors.muted;text:"Groups keep window placement intact. Full screen, paired layout and Show Desktop suspend grouping. Hidden and minimised windows remain hidden."}
                PanelLabel {visible:!!root.windowGroups && !root.windowGroups.separate_displays;width:parent.width;wrapMode:Text.WordWrap;text:"Window Groups require separate display spaces. Existing groups are retained and suspended."}
                DesktopComboBox {id:groupChoice;readonly property var currentChoice:model[currentIndex] || null;width:parent.width;textRole:"label";Accessible.name:"Window group";model:root.windowGroups?root.windowGroups.groups.map(g=>({label:"Group "+g.id+" · "+g.members.length+(g.members.length===1?" window":" windows")+(g.selected?" · selected":"")+(g.suspended?" · suspended":""),id:g.id,suspended:g.suspended})):[]}
                Row {spacing:8
                    DialogButton {objectName:"window-group-select";property var captured:null;text:"Select";Accessible.name:"Select window group "+(groupChoice.currentChoice?groupChoice.currentChoice.id:"");enabled:!root.busy && !!groupChoice.currentChoice && !groupChoice.currentChoice.suspended;onPressed:captured=(groupChoice.currentChoice?root.captureGroupAction("select",groupChoice.currentChoice.id):null);onCanceled:captured=null;onClicked:{const intent=captured || (groupChoice.currentChoice?root.captureGroupAction("select",groupChoice.currentChoice.id):null);captured=null;if(intent)root.send(intent);}}
                    DialogButton {objectName:"window-group-add";property var captured:null;text:"Add Window";Accessible.name:"Add selected window to group "+(groupChoice.currentChoice?groupChoice.currentChoice.id:"");enabled:!root.busy && groupChoice.count>0 && !!root.currentWindowGroup && !root.currentWindowGroup.suspended && !groupChoice.model[groupChoice.currentIndex].suspended;onPressed:captured=(groupChoice.currentChoice?root.captureGroupAction("add",groupChoice.currentChoice.id):null);onCanceled:captured=null;onClicked:{const intent=captured || (groupChoice.currentChoice?root.captureGroupAction("add",groupChoice.currentChoice.id):null);captured=null;if(intent)root.send(intent);}}
                    DialogButton {objectName:"window-group-create";property var captured:null;text:"New Group";Accessible.name:"Create group from selected window";enabled:!root.busy && !!root.currentWindowGroup && !root.currentWindowGroup.suspended && root.windowGroups.enabled;onPressed:{const g=root.windowGroups.groups.find(g=>g.members.some(m=>m.id===root.current.id && m.pid===root.current.pid));captured=g?root.captureGroupAction("remove",g.id):null;}onCanceled:captured=null;onClicked:{const intent=captured || (root.currentWindowGroup?root.captureGroupAction("remove",root.currentWindowGroup.id):null);captured=null;if(intent)root.send(intent);}}
                }
                PanelLabel {width:parent.width;wrapMode:Text.WordWrap;text:root.current?"Selected window: "+root.current.title:"Select a window in Overview first";textFormat:Text.PlainText;font.pixelSize:Theme.textRole("metadata")}
                DialogButton {text:"Done";Accessible.name:"Close window groups";onClicked:groupsDialog.close()}
            }
        }
        Dialog {id:removeDialog;title:"Remove Desktop";modal:true;anchors.centerIn:parent;width:Math.min(450,parent.width-40);closePolicy:Popup.CloseOnEscape
            background:Rectangle {radius:12;color:Theme.colors.background;border.color:Theme.outline}
            contentItem:Column {spacing:12
                PanelLabel {width:parent.width;text:root.removalCapture?"Remove "+root.removalCapture.source.label+"? Its windows move to the selected desktop. Nothing is closed.":"";wrapMode:Text.WordWrap}
                DesktopComboBox {id:removeDestination;width:parent.width;model:root.removalDestinations;textRole:"label";Accessible.name:"Desktop removal destination"}
                Row {spacing:8
                    DialogButton {text:"Cancel";onClicked:removeDialog.close()}
                    DialogButton {text:"Remove Desktop";destructive:true;enabled:!root.busy && root.removalDestinations.length>0;onClicked:{const capture=root.removalCapture,target=root.removalDestinations[removeDestination.currentIndex];root.send({action:"desktop-remove",source:capture.source.logicalId,destination:target.logicalId,revision:capture.revision,sessionEpoch:capture.sessionEpoch});removeDialog.close();}}
                }
            }
        }
    }
    Timer {interval:2000;running:root.visible && !desktopStrip.editing && !removeDialog.opened && !groupsDialog.opened && !(root.shell.groupMembershipDrag && root.shell.groupMembershipDrag.active);repeat:true;onTriggered:root.send({action:"status"})}
    Process {id:worker;stdinEnabled:true
        command:["lumen-window-overview"]
        onStarted:write(JSON.stringify(root.request)+"\n")
        stdout:StdioCollector {onStreamFinished:{try{const data=JSON.parse(text);root.error=data.error || "";if(data.created)root.managedDesktopId=data.created.logicalId;if(data.windows){const selectedId=root.draggedId || (root.current?root.current.id:0);root.windows=data.windows;root.windowGroups=data.windowGroups || null;const index=root.matches.findIndex(w=>w.id===selectedId);root.selected=root.draggedId?index:Math.max(0,index);if(root.draggedId && index<0)root.error="The dragged window is no longer available";root.workspaces=data.workspaces;root.outputs=data.outputs;root.removalSupported=!!(data.capabilities || {}).desktop_removal;root.revision=data.revision;root.sessionEpoch=data.sessionEpoch;
                    if(!root.workspaces.some(item=>item.logicalId===root.managedDesktopId)){const chosen=root.workspaces.find(item=>item.focused) || root.workspaces[0];root.managedDesktopId=chosen?chosen.logicalId:"";}
                    manageDesktop.currentIndex=Math.max(0,manageDesktop.model.findIndex(item=>item.logicalId===root.managedDesktopId));root.autoReorder=data.autoReorder;root.assignments=data.assignments;root.selected=Math.min(root.selected,Math.max(0,root.matches.length-1));}}catch(e){root.error="Unable to read desktop windows";}}}
        onExited:if(root.request.action!=="status" && root.visible)Qt.callLater(()=>root.send({action:"status"}))
    }
}

import QtQuick
import QtQuick.Controls
import QtQuick.Window
import Quickshell
import Quickshell.Io
import Quickshell.Wayland
PanelWindow {
    id:root
    required property var shell
    visible:false;color:"transparent"
    anchors {top:true;bottom:true;left:true;right:true}
    exclusionMode:ExclusionMode.Ignore
    WlrLayershell.namespace:"lumen-paired-windows"
    WlrLayershell.layer:WlrLayer.Overlay
    WlrLayershell.keyboardFocus:visible?WlrKeyboardFocus.Exclusive:WlrKeyboardFocus.None
    property double sourceId:0
    property double sourcePid:0
    property var source:null
    property var candidates:[]
    property int selected:-1
    property string side:"left"
    property string error:""
    property var request:({})
    property bool pendingRefresh:false
    readonly property bool busy:worker.running
    readonly property var current:candidates[selected] || null
    readonly property bool paired:!!source && !!source.pair.id
    readonly property bool survivor:!!source && source.pair.survivor===true
    readonly property bool canChoose:!!source && !survivor && (paired || source.pair.eligible===true)
    function open(id,placement){
        if(!Number.isSafeInteger(id) || id<=0)return;
        sourceId=id;sourcePid=0;source=null;candidates=[];selected=-1;side=placement==="right"?"right":"left";error="";
        shell.claimPopup("");visible=true;
        if(busy)pendingRefresh=true;else send("status");
        Qt.callLater(()=>content.forceActiveFocus());
    }
    function send(action,value){
        if(busy)return;
        const data={action:action,id:sourceId,pid:sourcePid};
        if(action==="create" || action==="replace"){if(!current)return;data.partner=current.id;data.partnerPid=current.pid;data.side=side;}
        if(action==="ratio")data.ratio=value;
        request=data;if(action!=="status")error="";worker.running=true;
    }
    Rectangle {anchors.fill:parent;color:"#55000000"}
    MouseArea {anchors.fill:parent;onClicked:root.visible=false}
    Rectangle {id:content;anchors.centerIn:parent;width:Math.min(520,root.width-32);height:Math.min(520,root.height-32);color:Theme.colors.background;radius:12;border.color:Theme.outline;focus:true
        MouseArea {anchors.fill:parent;onClicked:event=>event.accepted=true}
        PanelLabel {x:20;y:18;text:"Paired Windows";font.weight:Font.DemiBold}
        Keys.onEscapePressed:root.visible=false
        Keys.onReturnPressed:if(!root.busy && root.canChoose && root.current)root.send(root.paired?"replace":"create")
        PanelLabel {x:20;y:50;width:parent.width-40;text:root.source?root.source.title:"Reading captured window…";textFormat:Text.PlainText;font.weight:Font.DemiBold;elide:Text.ElideRight}
        PanelLabel {x:20;y:80;width:parent.width-40;wrapMode:Text.WordWrap;font.pixelSize:12;color:Theme.colors.muted;text:root.paired?"Choose a replacement partner, swap sides, or return this window to its desktop.":root.source && !root.canChoose && !root.survivor?"This window cannot be paired in its current state.":root.survivor?"This window remains full screen. Return it to its previous desktop when ready.":"Choose a compatible partner. The pair shares a separate full-screen desktop; drag its divider to resize."}
        ListView {id:list;x:20;y:140;width:parent.width-40;height:parent.height-300;clip:true;spacing:5;model:root.candidates;currentIndex:root.selected;enabled:!root.busy && root.canChoose;ScrollBar.vertical:DesktopScrollBar {}
            delegate:DialogButton {required property var modelData;required property int index;width:list.width;height:40;text:modelData.title;primary:index===root.selected;defaultAction:false;Accessible.name:modelData.title+" · "+modelData.app;onClicked:{root.selected=index;content.forceActiveFocus();}}
            PanelLabel {anchors.centerIn:parent;visible:!root.candidates.length;text:root.busy?"Reading windows…":"No compatible partner is available";color:Theme.colors.muted}
        }
        Row {x:20;y:parent.height-145;spacing:8
            DialogButton {text:"Keep on Left";visible:!root.paired && !root.survivor;enabled:!root.busy;primary:root.side==="left";defaultAction:false;onClicked:{root.side="left";content.forceActiveFocus();}}
            DialogButton {text:"Keep on Right";visible:!root.paired && !root.survivor;enabled:!root.busy;primary:root.side==="right";defaultAction:false;onClicked:{root.side="right";content.forceActiveFocus();}}
            DialogButton {text:"Swap Sides";visible:root.paired;enabled:!root.busy;onClicked:root.send("swap")}
            DialogButton {text:"Reset Division";visible:root.paired;enabled:!root.busy;onClicked:root.send("ratio",0.5)}
        }
        PanelLabel {x:20;y:parent.height-95;width:parent.width-40;text:root.error;textFormat:Text.PlainText;color:Theme.colors.muted;wrapMode:Text.WordWrap;font.pixelSize:12}
        Row {x:20;y:parent.height-50;spacing:8
            DialogButton {text:"Cancel";onClicked:root.visible=false}
            DialogButton {text:"Return to Desktop";visible:root.paired || root.survivor;enabled:!root.busy && !!root.source;onClicked:root.send("leave")}
            DialogButton {text:root.paired?"Replace Partner":"Create Pair";visible:!root.survivor;primary:true;defaultAction:false;enabled:!root.busy && root.canChoose && !!root.current;onClicked:root.send(root.paired?"replace":"create")}
        }
    }
    Timer {interval:2000;running:root.visible;repeat:true;onTriggered:root.send("status")}
    Process {id:worker;stdinEnabled:true
        command:["lumen-window-pair"]
        onStarted:write(JSON.stringify(root.request)+"\n")
        stdout:StdioCollector {onStreamFinished:{
            if(root.request.id!==root.sourceId)return;
            try {const data=JSON.parse(text);if(data.error)root.error=data.error;else if(root.request.action!=="status")root.error="";
                if(data.source){const selectedId=root.current?root.current.id:0;root.source=data.source;root.sourcePid=data.source.pid;root.screen=Quickshell.screens.find(screen=>screen.name===data.source.output) || Quickshell.screens[0];root.candidates=data.candidates;root.selected=root.candidates.findIndex(item=>item.id===selectedId);}
                else if(data.error){root.source=null;root.candidates=[];root.selected=-1;}
                if(data.ok && ["create","replace","leave"].includes(root.request.action))root.visible=false;
            }catch(error){root.error="Unable to read paired placement";root.source=null;root.candidates=[];root.selected=-1;}
        }}
        onExited:if(root.visible && (root.pendingRefresh || root.request.action!=="status")){root.pendingRefresh=false;Qt.callLater(()=>root.send("status"));}
    }
}

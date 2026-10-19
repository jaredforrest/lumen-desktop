import QtQuick
import QtQuick.Controls
import Quickshell
import Quickshell.Io
import Quickshell.Wayland
PanelWindow {
    id:root
    required property var shell
    visible:false;color:"transparent"
    anchors {top:true;bottom:true;left:true;right:true}
    exclusionMode:ExclusionMode.Ignore
    WlrLayershell.layer:WlrLayer.Overlay
    WlrLayershell.namespace:"lumen-force-quit"
    WlrLayershell.keyboardFocus:visible?WlrKeyboardFocus.Exclusive:WlrKeyboardFocus.None
    property string applicationName:""
    property string token:""
    property string phase:"closed"
    property string error:""
    property int generation:0
    property int windowCount:0
    property var pendingRequest:null
    function request(action,value){pendingRequest={generation:generation,action:action,value:value};drain();}
    function drain(){if(worker.running || !pendingRequest)return;const request=pendingRequest;pendingRequest=null;worker.requestGeneration=request.generation;worker.action=request.action;worker.command=["lumen-application-control","--request-id",String(request.generation),request.action,request.value];worker.running=true;}
    function show(name,windows){
        const payload=JSON.stringify(windows);
        ++generation;pendingRequest=null;token="";error="";applicationName=name;windowCount=0;
        screen=Quickshell.screens.find(screen=>screen.name===shell.preferredOutput()) || Quickshell.screens[0];
        shell.claimPopup("force-quit");visible=true;
        if(payload.length>65536){phase="error";error="The selected window list is too large. Close some windows and try again.";}
        else {phase="preparing";request("prepare",payload);}
        Qt.callLater(()=>cancelButton.forceActiveFocus());
    }
    function cancel(){
        if(phase==="confirming")return;
        ++generation;pendingRequest=null;visible=false;phase="closed";
        if(token)request("cancel",token);
        token="";
    }
    function confirm(){
        if(phase!=="ready" || worker.running || !token)return;
        phase="confirming";request("confirm",token);token="";
    }
    onVisibleChanged:if(!visible && shell.popupOwner==="force-quit")shell.popupOwner=""
    Connections {target:shell;function onDismissPopups(owner){if(owner!=="force-quit")root.cancel();}}
    MouseArea {anchors.fill:parent;onClicked:root.cancel()}
    Item {anchors.fill:parent;focus:true;Keys.onEscapePressed:root.cancel()}
    PanelSurface {
        Keys.onEscapePressed:root.cancel()
        anchors.centerIn:parent;width:Math.min(420,parent.width-32);height:250
        MouseArea {anchors.fill:parent}
        Column {anchors.fill:parent;anchors.margins:22;spacing:14
            PanelLabel {width:parent.width;text:"Force Quit "+root.applicationName+"?";font.pixelSize:17;font.weight:Font.DemiBold;wrapMode:Text.WordWrap;maximumLineCount:2;elide:Text.ElideRight}
            PanelLabel {width:parent.width;height:102;wrapMode:Text.WordWrap;font.pixelSize:13;text:root.error || (root.phase==="preparing"?"Checking the selected application's process identity…":root.phase==="confirming"?"Closing the selected application…":"Any unsaved changes will be lost. This closes "+root.windowCount+" selected window"+(root.windowCount===1?"":"s")+" and the application processes that own them.")}
            Row {width:parent.width;spacing:8
                ControlRow {id:cancelButton;width:(parent.width-8)/2;text:root.phase==="error"?"Close":"Cancel";enabled:root.phase!=="confirming";onClicked:root.cancel()
                    contentItem:PanelLabel {text:parent.text;horizontalAlignment:Text.AlignHCenter;verticalAlignment:Text.AlignVCenter}
                    background:Rectangle {radius:7;color:parent.hovered || parent.activeFocus?Theme.colors.hover:Theme.colors.field;border.width:parent.activeFocus?2:0;border.color:Theme.colors.accent}
                }
                ControlRow {width:(parent.width-8)/2;text:"Force Quit";enabled:root.phase==="ready" && !worker.running;onClicked:root.confirm();contentItem:PanelLabel {text:parent.text;color:parent.enabled?"#ff453a":Theme.colors.muted;horizontalAlignment:Text.AlignHCenter;verticalAlignment:Text.AlignVCenter} background:Rectangle {radius:7;color:parent.hovered || parent.activeFocus?Theme.colors.hover:Theme.colors.field;border.width:parent.activeFocus?2:0;border.color:Theme.colors.accent}}
            }
        }
    }
    Process {id:worker;property int requestGeneration:-1;property string action:"";property bool received:false
        onRunningChanged:if(running)received=false
        stdout:StdioCollector {onStreamFinished:{
            if(worker.action==="cancel" || worker.requestGeneration!==root.generation)return;
            try{
                const result=JSON.parse(text);if(result.requestId!==String(root.generation))return;worker.received=true;
                if(result.error){root.error=result.error;root.phase="error";return;}
                if(worker.action==="prepare"){
                    if(typeof result.token!=="string" || !/^[0-9a-f]{32}$/.test(result.token))throw Error("Invalid confirmation identity");
                    root.token=result.token;root.windowCount=result.windowCount;root.phase="ready";
                }else {root.phase="closed";root.visible=false;}
            }catch(error){root.error="The application control service did not respond correctly";root.phase="error";}
        }}
        onExited:(code,status)=>{if(action!=="cancel" && requestGeneration===root.generation && !received){root.error="The application control service is unavailable";root.phase="error";}Qt.callLater(root.drain);}
    }
}

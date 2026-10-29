import QtQuick
import QtQuick.Controls
import "FileShareIntent.js" as FileShareIntent

Popup {
    id:root
    objectName:"files-share-dialog"
    property var intent:null
    property var destination:null
    property var intentCurrent:function(value){return false;}
    property alias state:destinations
    modal:true;focus:true;padding:16
    width:Math.min(420,parent.width-32);height:Math.min(column.implicitHeight+32,parent.height-32)
    x:(parent.width-width)/2;y:(parent.height-height)/2
    closePolicy:destinations.working?Popup.NoAutoClose:Popup.CloseOnEscape
    onClosed:{intent=null;destination=null;}
    onOpened:Qt.callLater(()=>cancelButton.forceActiveFocus())
    function openFor(value){intent=value;destination=null;destinations.refresh();open();}
    ShareDestinationsState {
        id:destinations
        onWorkingChanged:if(working)Qt.callLater(()=>pendingLabel.forceActiveFocus())
        onFinished:result=>{
            if(result.state==="handoff"){
                if(!root.intentCurrent(root.intent))outcome={state:"error",detail:"The selected files changed. Cancel and open Share again."};
                else {AppLaunch.run(result.command);outcome={state:"opened",detail:result.detail};}
            }
            Qt.callLater(()=>cancelButton.forceActiveFocus());
        }
    }
    background:Rectangle {radius:12;color:Theme.colors.background;border.color:Theme.outline}
    contentItem:FocusFlickable {
        contentHeight:column.implicitHeight;clip:true
        ScrollBar.vertical:DesktopScrollBar {}
        Column {
            id:column;width:parent.width;spacing:12
            PanelLabel {width:parent.width;text:"Share";font.pixelSize:Theme.textRole("heading");font.bold:true}
            PanelLabel {width:parent.width;wrapMode:Text.Wrap;text:root.intent?root.intent.selection.length+" selected file"+(root.intent.selection.length===1?"":"s")+". Choose a destination; recipient consent stays in its application.":"Select files and open Share again."}
            PanelLabel {width:parent.width;visible:destinations.loading;text:"Finding destinations…"}
            PanelLabel {width:parent.width;wrapMode:Text.Wrap;visible:!destinations.loading && !destinations.destinations.length && !destinations.error;text:"No sharing destinations configured. Add a provider in the desktop integration settings."}
            Repeater {
                model:destinations.destinations
                ControlRow {
                    required property var modelData
                    objectName:"share-destination-"+modelData.id
                    width:column.width;height:Math.max(60,implicitHeight)
                    text:modelData.label
                    detail:modelData.state==="ready"?modelData.detail:(modelData.state==="offline"?"Offline — ":"Unavailable — ")+modelData.detail
                    enabled:!destinations.working && modelData.state==="ready"
                    selected:!!root.destination && root.destination.id===modelData.id
                    onClicked:root.destination=Object.assign({},modelData)
                }
            }
            PanelLabel {width:parent.width;wrapMode:Text.Wrap;visible:!!destinations.error;text:destinations.error;color:Theme.errorText}
            PanelLabel {objectName:"share-outcome";width:parent.width;wrapMode:Text.Wrap;visible:!!destinations.outcome;text:destinations.outcome?({completed:"Provider reports completion. ",cancelled:"Cancelled. ",offline:"Offline. ",error:"Sharing failed. ",opened:""}[destinations.outcome.state] || "")+destinations.outcome.detail:""}
            PanelLabel {id:pendingLabel;objectName:"share-pending";width:parent.width;wrapMode:Text.Wrap;visible:destinations.working;text:"Waiting for "+(root.destination?root.destination.label:"the provider")+". Complete or cancel its dialog; delivery is not confirmed yet."}
            Flow {
                width:parent.width;spacing:8
                DialogButton {id:cancelButton;objectName:"share-cancel";defaultAction:true;text:destinations.outcome?"Done":"Cancel";enabled:!destinations.working;onClicked:root.close()}
                DialogButton {objectName:"share-refresh";text:"Refresh";enabled:!destinations.loading && !destinations.working;onClicked:{root.destination=null;destinations.refresh();}}
                DialogButton {
                    objectName:"share-open";text:"Open Destination";primary:true;defaultAction:false;width:Math.min(parent.width,implicitWidth)
                    enabled:!!root.destination && !destinations.loading && !destinations.working && !destinations.outcome
                    onClicked:{
                        if(!root.intentCurrent(root.intent)){destinations.error="The selected files changed. Cancel and open Share again.";root.destination=null;return;}
                        const request=FileShareIntent.requestFor(root.intent,root.destination);
                        if(!request){destinations.error="The destination or selection is invalid. Cancel and select again.";return;}
                        destinations.invoke(request);
                    }
                }
            }
        }
    }
}

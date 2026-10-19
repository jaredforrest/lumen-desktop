import QtQuick
Column {
    id:root
    property bool active:false
    property var service:owned
    property var pending:null
    property string pendingAction:""
    spacing:10
    PortalPermissionState {id:owned;active:root.active && root.service===owned}
    function actionLabel(action){return action==="deny"?"Deny future requests":action==="reset"?"Ask next time":"Forget saved session";}
    function prepare(row,action){pending=JSON.parse(JSON.stringify(row));pendingAction=action;Qt.callLater(revealConfirmation);}
    function revealConfirmation(){
        let ancestor=parent;
        while(ancestor){
            if(ancestor.contentY!==undefined && ancestor.contentHeight!==undefined && ancestor.contentItem){
                const at=confirmationCard.mapToItem(ancestor.contentItem,0,0);
                ancestor.contentY=Math.max(0,Math.min(ancestor.contentHeight-ancestor.height,at.y-12));break;
            }
            ancestor=ancestor.parent;
        }
        cancelButton.forceActiveFocus();
    }
    Keys.onEscapePressed:if(pending!==null)cancel()
    function cancel(){pending=null;pendingAction="";}
    PanelLabel {text:"Saved portal permissions";font.weight:Font.DemiBold}
    PanelLabel {width:root.width;wrapMode:Text.WordWrap;text:"Saved decisions for apps using desktop portals. This does not list active capture or control all native app access. Microphone permissions are not managed here.";color:Theme.colors.muted}
    StatusMessage {width:root.width;visible:root.service.error!=="";text:root.service.error;severity:"error"}
    StatusMessage {width:root.width;visible:root.service.message!=="";text:root.service.message;severity:"success"}
    PanelLabel {width:root.width;visible:!root.service.busy && root.service.facts.available && root.service.facts.rows.length===0;text:"No saved decisions were returned from the readable portal scopes.";wrapMode:Text.WordWrap;color:Theme.colors.muted}
    Repeater {model:root.service.facts.coverage || []
        PanelLabel {required property var modelData;width:root.width;visible:modelData.state==="unavailable";text:modelData.capability+": this scope could not be read.";wrapMode:Text.WordWrap;color:Theme.colors.muted}
    }
    Rectangle {
        id:confirmationCard;objectName:"portal-permission-confirmation"
        visible:root.pending!==null;width:root.width;height:confirmation.implicitHeight+24;radius:10;color:Theme.colors.surface;border.color:Theme.colors.accent
        Column {id:confirmation;x:12;y:12;width:parent.width-24;spacing:8
            PanelLabel {width:parent.width;wrapMode:Text.WrapAnywhere;text:root.pending?root.actionLabel(root.pendingAction)+"?":"";font.weight:Font.DemiBold}
            PanelLabel {objectName:"portal-permission-exact-target";width:parent.width;wrapMode:Text.WrapAnywhere;text:root.pending?"App ID: "+root.pending.appId+"\nCapability: "+root.pending.capability+"\nResource: "+root.pending.resource+"\nCurrent decision: "+root.pending.decision:""}
            PanelLabel {width:parent.width;wrapMode:Text.WordWrap;text:"Only this saved portal entry will change. Other apps and resources stay unchanged. Existing capture may continue; stop it in the sharing app.";color:Theme.colors.muted}
            DialogButton {objectName:"portal-permission-confirm";text:"Confirm";destructive:true;enabled:root.pending!==null && !root.service.busy;onClicked:{const row=root.pending,action=root.pendingAction;root.cancel();root.service.apply(row,action);}}
            DialogButton {id:cancelButton;text:"Cancel";onClicked:root.cancel()}
        }
    }
    Repeater {model:root.service.facts.rows || []
        Rectangle {
            required property var modelData
            width:root.width;height:body.implicitHeight+24;radius:10;color:Theme.colors.surface;border.color:Theme.hairline
            Column {id:body;x:12;y:12;width:parent.width-24;spacing:6
                PanelLabel {width:parent.width;text:modelData.appName;wrapMode:Text.WrapAnywhere;font.weight:Font.DemiBold}
                PanelLabel {width:parent.width;text:modelData.appId || "No distinct app ID";wrapMode:Text.WrapAnywhere;font.pixelSize:12;color:Theme.colors.muted}
                PanelLabel {width:parent.width;text:modelData.capability+" · "+modelData.decision;wrapMode:Text.WordWrap}
                PanelLabel {width:parent.width;text:modelData.resource;wrapMode:Text.WrapAnywhere;font.pixelSize:12;color:Theme.colors.muted}
                PanelLabel {width:parent.width;text:modelData.detail;wrapMode:Text.WordWrap;color:Theme.colors.muted}
                Repeater {model:parent.parent.modelData.actions
                    DialogButton {required property string modelData;property string operation:modelData;property var entry:body.parent.modelData;objectName:"portal-permission-"+operation+"-"+entry.id;text:root.actionLabel(operation);enabled:!root.service.busy;Accessible.name:text+" for "+entry.appId+", "+entry.capability+", "+entry.resource;onClicked:root.prepare(entry,operation)}
                }
            }
        }
    }
    ControlRow {width:root.width;text:root.service.busy?"Reading portal decisions…":"Refresh portal decisions";enabled:!root.service.busy;onClicked:root.service.refresh()}
}

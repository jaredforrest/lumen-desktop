import QtQuick
import QtQuick.Controls
Popup {
    id:root
    objectName:"files-batch-rename"
    required property var service
    required property real ownerWidth
    required property real ownerHeight
    property var captured:[]
    property var rows:[]
    property string token:""
    property int requestId:0
    property bool waiting:false
    property bool executing:false
    property bool executable:false
    property int changeCount:0
    property string message:""
    property bool completed:false
    property bool acceptingReplies:false
    modal:true;focus:true
    width:Math.min(640,ownerWidth-24);height:Math.min(620,ownerHeight-24)
    x:(ownerWidth-width)/2;y:12;padding:16
    closePolicy:executing?Popup.NoAutoClose:Popup.CloseOnEscape
    background:Rectangle {radius:14;color:Theme.colors.background;border.color:Theme.outline}
    function begin(selection){
        if(executing)return;
        requestId=0;waiting=false;invalidate();acceptingReplies=true;captured=selection.map(row=>({path:row.path,identity:JSON.parse(JSON.stringify(row.identity))}));rows=[];message="";completed=false;
        renameMode.currentIndex=0;first.text="";second.text="";policy.currentIndex=0;open();Qt.callLater(()=>first.forceActiveFocus());
    }
    function invalidate(){if(token && service.ready)service.send("rename-cancel",{token:token});token="";executable=false;changeCount=0;}
    function changed(){if(executing)return;invalidate();requestId=0;waiting=false;rows=[];message="Preview the changed names before confirming.";completed=false;}
    function preview(){
        invalidate();waiting=true;message="Preparing preview…";
        requestId=service.send("rename-preview",{items:captured,rule:{mode:["replace","prefix","suffix"][renameMode.currentIndex],first:first.text,second:renameMode.currentIndex===0?second.text:""},policy:policy.currentIndex===0?"stop":"skip"});
    }
    function confirm(){
        if(!token || !executable || waiting || executing)return;
        const capturedToken=token;token="";executable=false;executing=true;waiting=true;message="Renaming the captured items…";
        requestId=service.send("rename-confirm",{token:capturedToken,confirmed:true});
    }
    onAboutToHide:{acceptingReplies=false;if(!executing){invalidate();requestId=0;waiting=false;}}
    Connections {target:root.service
        function onReadyChanged(){if(!root.service.ready){root.invalidate();root.requestId=0;root.waiting=false;root.executing=false;root.message="Files disconnected. Close this dialog and select the items again.";}}
        function onBatchRenameReady(reply){
            if(reply.id!==root.requestId || (!root.acceptingReplies && reply.action!=="rename-confirm")){if(reply.action==="rename-preview" && reply.token)root.service.send("rename-cancel",{token:reply.token});return;}
            root.waiting=false;root.requestId=0;
            if(reply.action==="rename-preview"){
                root.rows=reply.rows || [];root.token=reply.token || "";root.executable=reply.executable===true;root.changeCount=reply.count || 0;
                root.message=reply.error || (root.executable?root.changeCount+" changes ready. Existing items will never be replaced.":"Resolve invalid names or conflicts, or choose Skip Conflicts and preview again.");
            }else if(reply.action==="rename-confirm"){
                root.executing=false;root.completed=true;root.rows=reply.rows || root.rows;
                root.message=reply.error || (reply.ok?"Rename completed. Review the per-item results below.":"Rename stopped. Completed changes remain; review each item before starting a new preview.");
            }
        }
    }
    contentItem:ScrollView {
        id:scroll;clip:true;Accessible.role:Accessible.Dialog;Accessible.name:"Rename selected items";ScrollBar.vertical:DesktopScrollBar {}
        Column {
            width:scroll.availableWidth;spacing:9
            PanelLabel {width:parent.width;text:"Rename "+root.captured.length+" Items";font.pixelSize:18;font.weight:Font.DemiBold}
            PanelLabel {width:parent.width;text:"Names include extensions. Preview does not change files. A changed selection is never substituted.";wrapMode:Text.Wrap}
            DesktopComboBox {id:renameMode;objectName:"batch-rename-mode";width:parent.width;model:["Replace text","Add prefix","Add suffix"];Accessible.name:"Rename transformation";enabled:!root.waiting && !root.completed;onActivated:root.changed()}
            TextField {id:first;objectName:"batch-rename-first";width:parent.width;enabled:!root.waiting && !root.completed;placeholderText:renameMode.currentIndex===0?"Find text":"Text to add";Accessible.name:placeholderText;onTextEdited:root.changed()}
            TextField {id:second;objectName:"batch-rename-second";width:parent.width;visible:renameMode.currentIndex===0;enabled:!root.waiting && !root.completed;placeholderText:"Replace with";Accessible.name:placeholderText;onTextEdited:root.changed()}
            DesktopComboBox {id:policy;objectName:"batch-rename-conflicts";width:parent.width;model:["Stop before changes if there are conflicts","Skip conflicting items"];Accessible.name:"Rename collision policy";enabled:!root.waiting && !root.completed;onActivated:root.changed()}
            PanelLabel {width:parent.width;text:root.message;textFormat:Text.PlainText;wrapMode:Text.WrapAnywhere;Accessible.name:text}
            ListView {
                objectName:"batch-rename-preview";width:parent.width;height:Math.min(240,Math.max(80,contentHeight));clip:true;model:root.rows;spacing:5
                Accessible.name:"Before and after names"
                ScrollBar.vertical:DesktopScrollBar {}
                delegate:PanelLabel {
                    required property var modelData
                    width:ListView.view.width;text:modelData.before+" → "+modelData.after+" · "+modelData.status+(modelData.error?": "+modelData.error:"")
                    textFormat:Text.PlainText;wrapMode:Text.WrapAnywhere
                    Accessible.role:Accessible.ListItem;Accessible.name:text
                }
            }
            Flow {width:parent.width;spacing:8
                DialogButton {objectName:"batch-rename-cancel";text:root.completed?"Done":"Cancel";enabled:!root.executing;defaultAction:false;onClicked:root.close()}
                DialogButton {objectName:"batch-rename-preview-button";text:"Preview Changes";enabled:!root.waiting && !root.completed && first.text.length>0;defaultAction:false;onClicked:root.preview()}
                DialogButton {objectName:"batch-rename-confirm";text:"Rename "+root.changeCount+" Items";enabled:root.executable && !root.waiting;primary:true;defaultAction:false;onClicked:root.confirm()}
            }
        }
    }
}

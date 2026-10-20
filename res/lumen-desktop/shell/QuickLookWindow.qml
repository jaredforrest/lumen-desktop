import QtQuick
import QtQuick.Controls
import QtQuick.Window
import QtQuick.Dialogs
import Quickshell
import Quickshell.Io
FloatingWindow {
    id:root
    title:"Quick Look";visible:false;color:"transparent"
    implicitWidth:680;implicitHeight:520;minimumSize:Qt.size(400,300)
    function fit(){if(screen){implicitWidth=Math.max(400,Math.min(680,screen.width-32));implicitHeight=Math.max(300,Math.min(520,screen.height-130));}}
    property string path:""
    property int page:1
    property alias preview:previewContent
    property var result:previewState.result
    property var selectedIdentity:null
    property var menuIntent:null
    property var chooserIntent:null
    property var applications:[]
    property string actionMessage:""
    OwnedPreviewState {id:previewState;active:root.visible
        onReceived:value=>{if(!value.error && root.recognizeOnLoad){root.recognizeOnLoad=false;Qt.callLater(root.recognize);}}
        onActionReceived:value=>{
            root.actionMessage=value.error || value.warning || value.message || "";
            if(value.error)return;
            if(value.operation==="applications"){root.applications=value.applications || [];root.chooserIntent=Object.assign({},previewState.actionSelection,{choice:value.choice});openWith.open();}
            else if(value.operation==="ocr"){root.ocrText=value.text || value.message || "";recognized.open();}
            else if(value.operation==="cutout"){root.subjectMessage=value.message || "";if(value.subject){root.subjectPath=value.subject;root.subjectFacts=value.subjectFacts;root.subjectCapability=value.capability;subjectSave.currentFile="file://"+Quickshell.env("HOME")+"/Pictures/"+encodeURIComponent(root.path.split("/").pop().replace(/\.[^.]+$/,""))+"-subject.png";}subjectPopup.open();}
        }
    }
    ImageMarkupDialog {id:markup;service:previewState;ownerWidth:root.width;ownerHeight:root.height}
    property string ocrText:""
    property bool recognizeOnLoad:false
    function recognize(captured){const parameters={page:root.page};if(root.result.type==="media")parameters.seconds=previewContent.position/1000;previewState.invoke(captured || previewState.actionSelection,"ocr",parameters);}
    function extract(captured){previewState.invoke(captured,"cutout",{});}
    property string subjectPath:""
    property var subjectFacts:null
    property string subjectCapability:""
    property var subjectSaveIntent:null
    readonly property var subjectSelection:subjectPath && subjectFacts && previewState.actionSelection && subjectCapability===previewState.actionSelection.capability?{capability:subjectCapability,session:previewState.providerGeneration,choice:JSON.stringify(subjectFacts),source:subjectPath,facts:subjectFacts,path:root.path}:null
    property string subjectMessage:""
    property var returnTo:null
    readonly property bool keyActive:content.Window.window?content.Window.window.active:false
    function observeOwnedFocus(){if(!returnTo || !returnTo.shell.ownedContext)return;if(keyActive){returnTo.shell.ownedContext.focus(root,returnTo,"lumen-files","utility");returnTo.shell.activeFiles(returnTo);}else returnTo.shell.ownedContext.leave(root);}
    onKeyActiveChanged:observeOwnedFocus()
    onReturnToChanged:if(keyActive)observeOwnedFocus()
    Component.onDestruction:if(returnTo && returnTo.shell.ownedContext)returnTo.shell.ownedContext.close(root)
    function show(value,owner){if(owner && owner.shell.claimPopup)owner.shell.claimPopup("");markup.close();path=value;subjectPath="";subjectMessage="";actionMessage="";chooserIntent=null;menuIntent=null;subjectSave.close();subjectSaveIntent=null;openWith.close();recognized.close();subjectPopup.close();page=1;returnTo=owner;selectedIdentity=owner && owner.current && owner.current.path===value?owner.current.identity:null;visible=true;previewState.request(path,page,selectedIdentity,null);if(owner)owner.shell.focusOwnedWindow("Quick Look");Qt.callLater(()=>{content.forceActiveFocus();if(content.Window.window)content.Window.window.requestActivate();});}
    function saveSubject(captured){if(captured){subjectSaveIntent=JSON.parse(JSON.stringify(captured));subjectSave.open();}}
    function subjectAction(action,intent,destination){
        if(!intent || !previewState.actionSelection || intent.capability!==previewState.actionSelection.capability)return;
        subject.requestPath=intent.path;subject.requestCapability=intent.capability;subject.requestSource=intent.source;subject.requestFacts=JSON.stringify(intent.facts);subject.request={action:action,source:intent.source,sourceFacts:intent.facts,overwrite:true,destination:destination || ""};subject.running=true;
    }
    function refresh(){previewState.request(path,page,selectedIdentity,result.sourcePath===path?result.sourceToken:null);}
    onVisibleChanged:if(visible)fit();else {if(returnTo && returnTo.shell.ownedContext)returnTo.shell.ownedContext.close(root);subjectSave.close();recognizeOnLoad=false;subject.running=false;openWith.close();recognized.close();subjectPopup.close();if(returnTo)Qt.callLater(()=>returnTo.focusContents());}
    Rectangle {id:content;anchors.fill:parent;radius:Theme.nativeWindowFrame?0:12;color:Theme.colors.background;border.color:Theme.outline;focus:true
        Keys.onEscapePressed:root.visible=false
        Keys.onSpacePressed:root.visible=false
        Keys.onPressed:event=>{if(event.key===Qt.Key_P && root.result.type==="media"){previewContent.togglePlayback();event.accepted=true;}}
        Keys.onUpPressed:if(root.page>1){root.page--;root.refresh();}
        Keys.onDownPressed:if(root.page<(root.result.pages || 1)){root.page++;root.refresh();}
        Keys.onLeftPressed:if(root.returnTo)root.returnTo.quickLookStep(-1)
        Keys.onRightPressed:if(root.returnTo)root.returnTo.quickLookStep(1)
        MouseArea {width:parent.width;height:48;onPressed:if(content.Window.window)content.Window.window.startSystemMove()}
        WindowControls {windowName:"Quick Look";canMinimize:false;canFullscreen:false;windowActive:content.Window.window?content.Window.window.active:false;onTriggered:root.visible=false}
        PanelLabel {x:94;y:14;visible:!Theme.nativeWindowFrame;width:Math.max(40,parent.width-236);height:24;text:root.result.title || root.path.split("/").pop();font.weight:Font.DemiBold;verticalAlignment:Text.AlignVCenter}
        CapturedActionButton {anchors.right:more.left;anchors.rightMargin:8;y:12;width:70;text:"Open";selection:previewState.actionSelection;enabled:!!selection && !previewState.actionBusy;onActivatedIntent:captured=>previewState.invoke(captured,"open",{})}
        DialogButton {id:more;anchors.right:parent.right;anchors.rightMargin:14;y:12;width:32;text:"•••";Accessible.name:"Preview actions";onClicked:previewActions.popup()}
        Menu {id:previewActions
            onAboutToShow:root.menuIntent=previewState.actionSelection
            MenuItem {text:"Annotate a Copy…";enabled:!!root.menuIntent && !previewState.actionBusy && root.result.type==="image" && ["image/png","image/jpeg","image/webp"].includes(root.result.mime);onTriggered:markup.begin(root.result,root.menuIntent)}
            MenuItem {text:"Open With…";enabled:!!root.menuIntent && !previewState.actionBusy;onTriggered:previewState.invoke(root.menuIntent,"applications",{})}
            MenuItem {text:previewState.actionOperation==="ocr"?"Cancel Text Recognition":"Copy Text…";enabled:previewState.actionOperation==="ocr" || (!!root.menuIntent && !previewState.actionBusy && ["image","media"].includes(root.result.type));onTriggered:{if(previewState.actionOperation==="ocr")previewState.cancelContent();else root.recognize(root.menuIntent);}}
            MenuItem {text:previewState.actionOperation==="cutout"?"Cancel Extraction":"Lift Subject…";enabled:previewState.actionOperation==="cutout" || (!!root.menuIntent && !previewState.actionBusy && (root.result.mime || "").startsWith("image/"));onTriggered:{if(previewState.actionOperation==="cutout")previewState.cancelContent();else root.extract(root.menuIntent);}}
        }
        Popup {id:openWith;onClosed:root.chooserIntent=null;parent:content;anchors.centerIn:parent;width:Math.min(380,content.width-32);height:Math.min(360,content.height-32);modal:true;focus:true;padding:12
            background:Rectangle {radius:12;color:Theme.colors.background;border.color:Theme.outline}
            contentItem:ListView {clip:true;model:root.applications;ScrollBar.vertical:DesktopScrollBar {}
                delegate:CapturedActionButton {required property var modelData;width:ListView.view.width;text:modelData.name;selection:root.chooserIntent?Object.assign({},root.chooserIntent,{application:modelData.id}):null;enabled:!!selection && !previewState.actionBusy;onActivatedIntent:captured=>{previewState.invoke(captured,"open-with",{},captured.application);openWith.close();}}
                PanelLabel {width:parent.width;visible:root.applications.length===0;text:"No applications registered for this file";wrapMode:Text.WordWrap}
            }
        }
        PanelLabel {x:110;y:42;width:parent.width-140;text:root.actionMessage;visible:!!text;elide:Text.ElideRight;color:Theme.colors.muted;font.pixelSize:11}
        Popup {id:subjectPopup;parent:content;anchors.centerIn:parent;width:Math.min(600,content.width-24);height:Math.min(400,content.height-24);modal:true;focus:true;padding:16
            background:Rectangle {radius:12;color:Theme.colors.background;border.color:Theme.outline}
            contentItem:Column {spacing:8
                PanelLabel {text:"Extracted Subject";font.weight:Font.DemiBold}
                Image {width:parent.width;height:Math.max(40,subjectPopup.height-150);source:root.subjectPath?"file://"+root.subjectPath:"";fillMode:Image.PreserveAspectFit;asynchronous:true}

                Row {spacing:8
                    CapturedActionButton {text:"Copy";selection:root.subjectSelection;enabled:!!selection && !subject.running;onActivatedIntent:captured=>root.subjectAction("copy-subject",captured)}
                    CapturedActionButton {text:"Save…";selection:root.subjectSelection;enabled:!!selection && !subject.running;onActivatedIntent:captured=>root.saveSubject(captured)}
                    DialogButton {text:"Done";onClicked:subjectPopup.close()}
                }
                PanelLabel {text:root.subjectMessage;width:parent.width;wrapMode:Text.WrapAnywhere;font.pixelSize:11;color:Theme.colors.muted}
            }
        }
        Popup {id:recognized;parent:content;anchors.centerIn:parent;width:Math.min(580,content.width-24);height:Math.min(380,content.height-24);modal:true;focus:true;padding:16
            background:Rectangle {radius:12;color:Theme.colors.background;border.color:Theme.outline}
            contentItem:Column {spacing:10
                PanelLabel {text:"Recognized Text";font.weight:Font.DemiBold}
                ScrollView {width:parent.width;height:recognized.height-115
                    TextArea {text:root.ocrText;readOnly:true;selectByMouse:true;wrapMode:TextEdit.Wrap;color:Theme.colors.text;font.family:Theme.font;Accessible.name:"Recognized text";background:Rectangle {color:Theme.colors.field;radius:6}}
                }
                Row {spacing:8
                    DialogButton {text:"Copy";onClicked:Quickshell.execDetached(["wl-copy","--",root.ocrText])}
                    DialogButton {text:"Done";onClicked:recognized.close()}
                }
            }
        }
        FilePreview {id:previewContent;resourceRetirement:previewState.resourceRetirement;x:20;y:62;width:parent.width-40;height:parent.height-78;result:root.result;active:root.visible;loading:previewState.busy;onPageRequested:number=>{root.page=number;root.refresh();}}

    }
    FileDialog {id:subjectSave;title:"Save Extracted Subject";fileMode:FileDialog.SaveFile;nameFilters:["PNG image (*.png)"];defaultSuffix:"png"
        onAccepted:root.subjectAction("save-subject",root.subjectSaveIntent,decodeURIComponent(selectedFile.toString().replace(/^file:\/\//,"")))
    }
    Process {id:subject;property string requestPath;property string requestCapability;property string requestSource;property string requestFacts;property var request;stdinEnabled:true
        command:[Quickshell.env("GLOBAL_MENU_PYTHON") || "python3",Qt.resolvedUrl("context_tools.py").toString().replace("file://","")]
        onStarted:write(JSON.stringify(request)+"\n")
        stdout:StdioCollector {onStreamFinished:{if(!root.visible || root.path!==subject.requestPath || !previewState.actionSelection || subject.requestCapability!==previewState.actionSelection.capability || root.subjectPath!==subject.requestSource || JSON.stringify(root.subjectFacts)!==subject.requestFacts)return;try{const data=JSON.parse(text);root.subjectMessage=data.error || data.message || "";if(data.subject){root.subjectPath=data.subject;subjectSave.currentFile="file://"+Quickshell.env("HOME")+"/Pictures/"+encodeURIComponent(root.path.split("/").pop().replace(/\.[^.]+$/,"")+"-subject.png");}subjectPopup.open();}catch(e){root.subjectMessage="Unable to extract subject";subjectPopup.open();}}}
    }
}

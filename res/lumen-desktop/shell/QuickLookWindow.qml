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
    OwnedPreviewState {id:previewState;active:root.visible;onReceived:value=>{if(!value.error && root.recognizeOnLoad){root.recognizeOnLoad=false;Qt.callLater(root.recognize);}}}
    property string ocrText:""
    property bool recognizeOnLoad:false
    function recognize(){ocr.requestPath=root.path;ocr.request={action:"ocr",path:root.path,page:root.page};if(root.result.type==="media")ocr.request.seconds=previewContent.position/1000;ocr.running=true;}
    property string subjectPath:""
    property string subjectMessage:""
    property var returnTo:null
    readonly property bool keyActive:content.Window.window?content.Window.window.active:false
    function observeOwnedFocus(){if(!returnTo || !returnTo.shell.ownedContext)return;if(keyActive){returnTo.shell.ownedContext.focus(root,returnTo,"lumen-files","utility");returnTo.shell.activeFiles(returnTo);}else returnTo.shell.ownedContext.leave(root);}
    onKeyActiveChanged:observeOwnedFocus()
    onReturnToChanged:if(keyActive)observeOwnedFocus()
    Component.onDestruction:if(returnTo && returnTo.shell.ownedContext)returnTo.shell.ownedContext.close(root)
    function show(value,owner){if(owner && owner.shell.claimPopup)owner.shell.claimPopup("");path=value;subjectPath="";subjectMessage="";page=1;returnTo=owner;selectedIdentity=owner && owner.current && owner.current.path===value?owner.current.identity:null;visible=true;previewState.request(path,page,selectedIdentity,null);if(owner)owner.shell.focusOwnedWindow("Quick Look");Qt.callLater(()=>{content.forceActiveFocus();if(content.Window.window)content.Window.window.requestActivate();});}
    function saveSubject(){if(subjectPath)subjectSave.open();}
    function refresh(){previewState.request(path,page,selectedIdentity,result.sourcePath===path?result.sourceToken:null);}
    onVisibleChanged:if(visible)fit();else {if(returnTo && returnTo.shell.ownedContext)returnTo.shell.ownedContext.close(root);subjectSave.close();recognizeOnLoad=false;ocr.running=false;subject.running=false;if(returnTo)Qt.callLater(()=>returnTo.focusContents());}
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
        DialogButton {anchors.right:more.left;anchors.rightMargin:8;y:12;width:70;text:"Open";onClicked:AppLaunch.run(["xdg-open","file://"+root.path.split("/").map(encodeURIComponent).join("/")])}
        DialogButton {id:more;anchors.right:parent.right;anchors.rightMargin:14;y:12;width:32;text:"•••";Accessible.name:"Preview actions";onClicked:previewActions.popup()}
        Menu {id:previewActions
            MenuItem {text:ocr.running?"Cancel Text Recognition":"Copy Text…";enabled:["image","media"].includes(root.result.type);onTriggered:{if(ocr.running)ocr.running=false;else root.recognize();}}
            MenuItem {text:subject.running?"Cancel Extraction":"Lift Subject…";enabled:(root.result.mime || "").startsWith("image/");onTriggered:{if(subject.running)subject.running=false;else {subject.requestPath=root.path;subject.request={action:"cutout",path:root.path};subject.running=true;}}}
        }
        Popup {id:subjectPopup;parent:content;anchors.centerIn:parent;width:Math.min(600,content.width-24);height:Math.min(400,content.height-24);modal:true;focus:true;padding:16
            background:Rectangle {radius:12;color:Theme.colors.background;border.color:Theme.outline}
            contentItem:Column {spacing:8
                PanelLabel {text:"Extracted Subject";font.weight:Font.DemiBold}
                Image {width:parent.width;height:Math.max(40,subjectPopup.height-150);source:root.subjectPath?"file://"+root.subjectPath:"";fillMode:Image.PreserveAspectFit;asynchronous:true}

                Row {spacing:8
                    DialogButton {text:"Copy";enabled:!!root.subjectPath && !subject.running;onClicked:{subject.request={action:"copy-subject",source:root.subjectPath};subject.running=true;}}
                    DialogButton {text:"Save…";enabled:!!root.subjectPath && !subject.running;onClicked:root.saveSubject()}
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
        FilePreview {id:previewContent;x:20;y:62;width:parent.width-40;height:parent.height-78;result:root.result;active:root.visible;loading:previewState.busy;onPageRequested:number=>{root.page=number;root.refresh();}}

    }
    FileDialog {id:subjectSave;title:"Save Extracted Subject";fileMode:FileDialog.SaveFile;nameFilters:["PNG image (*.png)"];defaultSuffix:"png"
        onAccepted:{subject.request={action:"save-subject",source:root.subjectPath,overwrite:true,destination:decodeURIComponent(selectedFile.toString().replace(/^file:\/\//,""))};subject.running=true;}
    }
    Process {id:subject;property string requestPath;property var request;stdinEnabled:true
        command:[Quickshell.env("GLOBAL_MENU_PYTHON") || "python3",Qt.resolvedUrl("context_tools.py").toString().replace("file://","")]
        onStarted:write(JSON.stringify(request)+"\n")
        stdout:StdioCollector {onStreamFinished:{if(!root.visible || root.path!==subject.requestPath)return;try{const data=JSON.parse(text);root.subjectMessage=data.error || data.message || "";if(data.subject){root.subjectPath=data.subject;subjectSave.currentFile="file://"+Quickshell.env("HOME")+"/Pictures/"+encodeURIComponent(root.path.split("/").pop().replace(/\.[^.]+$/,"")+"-subject.png");}subjectPopup.open();}catch(e){root.subjectMessage="Unable to extract subject";subjectPopup.open();}}}
    }
    Process {id:ocr;property string requestPath;property var request;stdinEnabled:true
        command:[Quickshell.env("GLOBAL_MENU_PYTHON") || "python3",Qt.resolvedUrl("context_tools.py").toString().replace("file://","")]
        onStarted:write(JSON.stringify(request)+"\n")
        stdout:StdioCollector {onStreamFinished:{if(!root.visible || root.path!==ocr.requestPath)return;try{const data=JSON.parse(text);root.ocrText=data.error || data.text || data.message;recognized.open();}catch(e){root.ocrText="Text recognition failed";recognized.open();}}}
    }
}

import QtQuick
import QtQuick.Controls
import QtQuick.Dialogs
Popup {
    id:root
    required property var service
    property real ownerWidth:680
    property real ownerHeight:520
    property var captured:null
    property var destination:null
    property int editGeneration:0
    property string sourceUri:""
    property string pendingUri:""
    property bool awaitingInfo:false
    property bool supported:false
    property string sourcePath:""
    property var strokes:[]
    property var draft:[]
    property var draftStyle:({tool:"pen",color:"#ff3b30"})
    property string message:""
    property string tool:"pen"
    property string ink:"#ff3b30"
    readonly property bool current:!!captured && !!service.actionSelection && captured.capability===service.actionSelection.capability && captured.session===service.actionSelection.session
    readonly property bool imageReady:supported && image.status===Image.Ready
    readonly property bool saving:service.actionBusy && service.actionOperation==="image-markup"
    readonly property int pointCount:strokes.reduce((count,stroke)=>count+stroke.points.length,0)
    property bool drawing:false
    modal:true;focus:true;closePolicy:saving?Popup.NoAutoClose:Popup.CloseOnEscape
    width:Math.min(640,ownerWidth-16);height:Math.min(480,ownerHeight-16)
    x:(ownerWidth-width)/2;y:(ownerHeight-height)/2
    padding:12
    background:PanelSurface {color:Theme.colors.background;radius:12}
    function begin(result,selection){
        if(!selection || !result || result.type!=="image" || !["image/png","image/jpeg","image/webp"].includes(result.mime))return false;
        ++editGeneration;captured=Object.assign({},selection);sourceUri="";pendingUri=result.uri;supported=false;awaitingInfo=true;sourcePath=result.path;strokes=[];draft=[];message="";if(!service.invoke(captured,"image-markup-info",{})){awaitingInfo=false;return false;}return true;
    }
    function retireChooser(dialog){if(!dialog || dialog.retired)return;dialog.retired=true;if(destination===dialog)destination=null;dialog.close();Qt.callLater(()=>dialog.destroy());}
    function cancel(){if(saving)return;retireChooser(destination);close();}
    function captureSave(){return imageReady && current && !saving && !drawing && strokes.length?{generation:editGeneration,captured:Object.assign({},captured),strokes:JSON.parse(JSON.stringify(strokes))}:null;}
    function chooseDestination(){
        const intent=captureSave();if(!intent || destination)return;
        destination=destinationComponent.createObject(root,{capturedIntent:intent});
        if(!destination){message="The Save Copy chooser is unavailable";return;}
        destination.currentFile="file://"+sourcePath.replace(/\.[^/.]+$/,"").split("/").map(encodeURIComponent).join("/")+"-annotated.png";destination.open();
    }
    function addRectangle(){if(imageReady && current && !saving && strokes.length<512 && pointCount<=8190){strokes=strokes.concat([{tool:"rectangle",color:ink,width:.005,points:[[.2,.2],[.8,.8]]}]);drawingCanvas.requestPaint();}}
    function saveTo(value,intent){
        if(!visible || !imageReady || !current || saving || drawing || !intent || intent.generation!==editGeneration || !intent.captured || intent.captured.capability!==captured.capability || intent.captured.session!==captured.session)return false;
        message="";return service.invoke(intent.captured,"image-markup",{destination:value,strokes:intent.strokes});
    }
    onClosed:{++editGeneration;sourceUri="";pendingUri="";awaitingInfo=false;supported=false;captured=null;strokes=[];draft=[];drawing=false;retireChooser(destination);}
    onCurrentChanged:if(!current){sourceUri="";pendingUri="";awaitingInfo=false;Qt.callLater(()=>{if(root.visible && !root.current)root.close();});}
    Connections {target:root.service;function onActionReceived(value){
        if(!root.captured || value.capability!==root.captured.capability)return;
        if(value.operation==="image-markup-info" && root.awaitingInfo && root.current){root.awaitingInfo=false;root.supported=value.supported===true && !value.error;root.sourceUri=root.supported?root.pendingUri:"";root.pendingUri="";root.message=value.error || "";root.open();return;}
        if(!root.visible || value.operation!=="image-markup")return;
        root.message=value.error || value.warning || value.message || "";
        if(value.saved===true)root.close();
    }}
    Component {id:destinationComponent
        FileDialog {property var capturedIntent:null;property bool retired:false;title:"Save Annotated Copy";fileMode:FileDialog.SaveFile;nameFilters:["PNG image (*.png)"];options:FileDialog.DontConfirmOverwrite
            onAccepted:{if(!retired)root.saveTo(selectedFile.toString(),capturedIntent);root.retireChooser(this);}
            onRejected:root.retireChooser(this)
        }
    }
    contentItem:Column {
        spacing:8
        PanelLabel {id:heading;text:"Annotate a Copy";font.pixelSize:16;font.bold:true}
        PanelLabel {id:subtitle;width:parent.width;text:"Still PNG, JPEG and WebP images. Save Copy creates a new PNG; the original stays unchanged.";wrapMode:Text.WordWrap;font.pixelSize:11}
        Flow {id:toolbar;width:parent.width;spacing:6
            DialogButton {text:"Draw";width:65;enabled:root.imageReady && root.current && !root.saving;onClicked:root.tool="pen";Accessible.description:"Draw freehand on the image"}
            DialogButton {text:"Rectangle";width:95;enabled:root.imageReady && root.current && !root.saving;onClicked:root.tool="rectangle"}
            DialogButton {objectName:"markup-add-rectangle";text:"Add Rectangle";width:115;enabled:root.imageReady && root.current && !root.saving;onClicked:root.addRectangle();Accessible.description:"Add a centred rectangle without pointer drawing"}
            DesktopComboBox {width:85;model:["Red","Blue","Black"];enabled:!root.saving;Accessible.name:"Annotation colour";onActivated:index=>root.ink=["#ff3b30","#007aff","#000000"][index]}
            DialogButton {objectName:"markup-undo";text:"Undo";width:60;enabled:root.strokes.length>0 && !root.saving;onClicked:{root.strokes=root.strokes.slice(0,-1);drawingCanvas.requestPaint();}}
        }
        Rectangle {
            id:surface;width:parent.width;height:Math.max(30,root.availableHeight-heading.height-subtitle.height-toolbar.height-feedback.height-footer.height-40);color:Theme.colors.surface;clip:true
            Image {id:image;anchors.fill:parent;source:root.visible && root.supported && root.current?root.sourceUri:"";sourceSize:Qt.size(2400,2400);autoTransform:false;fillMode:Image.PreserveAspectFit;asynchronous:true;cache:false;onStatusChanged:{drawingCanvas.requestPaint();if(status===Image.Error)root.message="This image could not be displayed for markup";}}
            PreviewResourceConsumer {manager:root.service.resourceRetirement;sourceObjects:[image]}
            Canvas {id:drawingCanvas;anchors.fill:parent
                onPaint:{
                    const ctx=getContext("2d");ctx.clearRect(0,0,width,height);const w=image.paintedWidth,h=image.paintedHeight,x=(width-w)/2,y=(height-h)/2;
                    for(const stroke of root.strokes.concat(root.draft.length?[{tool:root.draftStyle.tool,color:root.draftStyle.color,width:.005,points:root.draft}]:[])){
                        ctx.strokeStyle=stroke.color;ctx.lineWidth=stroke.width*Math.min(w,h);ctx.lineJoin="round";ctx.lineCap="round";ctx.beginPath();
                        if(stroke.tool==="rectangle" && stroke.points.length>1){const a=stroke.points[0],b=stroke.points[1];ctx.rect(x+a[0]*w,y+a[1]*h,(b[0]-a[0])*w,(b[1]-a[1])*h);}else for(let i=0;i<stroke.points.length;i++){const p=stroke.points[i];if(i===0)ctx.moveTo(x+p[0]*w,y+p[1]*h);else ctx.lineTo(x+p[0]*w,y+p[1]*h);}ctx.stroke();
                    }
                }
            }
            MouseArea {objectName:"markup-drawing-surface";anchors.fill:parent;enabled:root.imageReady && root.current && !root.saving && image.status===Image.Ready
                function point(mouse){const w=image.paintedWidth,h=image.paintedHeight,x=(width-w)/2,y=(height-h)/2;return w>0 && h>0 && mouse.x>=x && mouse.x<=x+w && mouse.y>=y && mouse.y<=y+h?[(mouse.x-x)/w,(mouse.y-y)/h]:null;}
                onPressed:mouse=>{const p=point(mouse);if(!p || root.strokes.length>=512 || root.pointCount>8190){mouse.accepted=false;return;}root.drawing=true;root.draftStyle={tool:root.tool,color:root.ink};root.draft=[p,p];drawingCanvas.requestPaint();}
                onPositionChanged:mouse=>{if(!root.drawing)return;const p=point(mouse);if(!p)return;if(root.draftStyle.tool==="rectangle")root.draft=[root.draft[0],p];else if(root.pointCount+root.draft.length<8192)root.draft=root.draft.concat([p]);drawingCanvas.requestPaint();}
                onReleased:{if(root.drawing){root.strokes=root.strokes.concat([{tool:root.draftStyle.tool,color:root.draftStyle.color,width:.005,points:root.draft}]);root.draft=[];root.drawing=false;drawingCanvas.requestPaint();}}
                onCanceled:{root.draft=[];root.drawing=false;drawingCanvas.requestPaint();}
            }
        }
        StatusMessage {id:feedback;width:parent.width;text:root.message}
        Row {id:footer;spacing:8;layoutDirection:Qt.RightToLeft;width:parent.width
            DialogButton {objectName:"markup-save-copy";text:root.saving?"Saving…":"Save Copy…";width:110;primary:true;enabled:root.imageReady && root.current && !root.saving && !root.drawing && root.strokes.length>0;onClicked:root.chooseDestination()}
            DialogButton {objectName:"markup-cancel";text:"Cancel";width:85;enabled:!root.saving;onClicked:root.cancel()}
        }
    }
}

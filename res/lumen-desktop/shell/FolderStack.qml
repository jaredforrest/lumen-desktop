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
    WlrLayershell.namespace:"lumen-folder-stack"
    WlrLayershell.keyboardFocus:WlrKeyboardFocus.Exclusive
    property string folder:""
    property var rows:[]
    property int total:0
    property bool limited:false
    property string error:""
    property int selected:0
    function show(path,outputName){folder=path;rows=[];error="";selected=0;screen=Quickshell.screens.find(s=>s.name===outputName) || Quickshell.screens[0];shell.claimPopup("folder-stack");visible=true;worker.running=true;}
    function openItem(){if(!rows[selected])return;const item=rows[selected];visible=false;if(item.directory)shell.openFiles(item.path);else AppLaunch.run(["xdg-open",item.uri]);}
    onVisibleChanged:if(!visible && shell.popupOwner==="folder-stack")shell.claimPopup("")
    Connections {target:shell;function onDismissPopups(owner){if(owner!=="folder-stack")root.visible=false;}}
    MouseArea {anchors.fill:parent;onClicked:root.visible=false}
    Item {anchors.fill:parent;focus:true
        Keys.onEscapePressed:root.visible=false
        Keys.onUpPressed:root.selected=Math.max(0,root.selected-1)
        Keys.onDownPressed:root.selected=Math.min(root.rows.length-1,root.selected+1)
        Keys.onReturnPressed:root.openItem()
    }
    PanelSurface {x:Math.max(8,parent.width-width-32);y:Math.max(30,parent.height-height-96);width:Math.min(350,parent.width-16);height:Math.min(parent.height-136,Math.max(160,100+root.rows.length*32))
        MouseArea {anchors.fill:parent}
        Column {anchors.fill:parent;anchors.margins:14;spacing:5
            PanelLabel {text:"Downloads"+(root.total?" · "+root.total+(root.limited?"+":""):"");font.weight:Font.DemiBold;height:24}
            ListView {id:items;width:parent.width;height:Math.max(0,parent.height-62);clip:true;model:root.rows;currentIndex:root.selected;onCurrentIndexChanged:positionViewAtIndex(currentIndex,ListView.Contain)
                delegate:ControlRow {required property var modelData;required property int index;width:items.width;height:32;leftPadding:38;text:modelData.name;Image {x:5;y:5;width:22;height:22;asynchronous:true;sourceSize:Qt.size(44,44);fillMode:Image.PreserveAspectFit;property bool failed:false;onStatusChanged:if(status===Image.Error)failed=true;source:!failed && !modelData.directory && modelData.kind.startsWith("image/") && modelData.size<16777216?modelData.uri:Quickshell.iconPath(modelData.directory?"folder":modelData.kind.startsWith("image/")?"image-x-generic":modelData.kind.startsWith("video/")?"video-x-generic":"text-x-generic")}selected:root.selected===index;onClicked:{root.selected=index;root.openItem();}}
                PanelLabel {visible:root.rows.length===0;width:parent.width;text:root.error || (worker.running?"Loading…":"No downloads yet");wrapMode:Text.WordWrap;color:Theme.colors.muted;font.pixelSize:12}
            }
            ControlRow {width:parent.width;height:28;text:root.total>root.rows.length || root.limited?"Show All in Files…":"Open in Files";onClicked:{root.visible=false;root.shell.openFiles(root.folder);}}
        }
    }
    Process {id:worker;stdinEnabled:true;command:[Quickshell.env("GLOBAL_MENU_PYTHON") || "python3",Qt.resolvedUrl("files.py").toString().replace("file://","")]
        onStarted:write(JSON.stringify({action:"column-list",path:root.folder,sort:"modified",descending:true})+"\n")
        stdout:SplitParser {onRead:line=>{try{const result=JSON.parse(line);root.total=(result.rows || []).length;root.limited=!!result.limited;root.rows=(result.rows || []).slice(0,14);root.error=result.error || "";}catch(e){root.error="Downloads are unavailable";}worker.running=false;}}
    }
}

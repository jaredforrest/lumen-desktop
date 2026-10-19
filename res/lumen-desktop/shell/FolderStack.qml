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
    WlrLayershell.keyboardFocus:visible?WlrKeyboardFocus.Exclusive:WlrKeyboardFocus.None
    property string folder:""
    property var rows:[]
    property int total:0
    property bool limited:false
    property string error:""
    property int selected:0
    property int generation:0
    property string sort:"modified"
    property bool descending:true
    property string presentation:"auto"
    readonly property bool grid:presentation==="grid" || (presentation==="auto" && rows.length>10)
    readonly property string folderName:folder.split("/").filter(Boolean).pop() || "/"
    function show(path,outputName,options){
        ++generation;worker.running=false;folder=path;rows=[];error="";selected=0;
        sort=options && options.sort || "modified";descending=options?options.descending!==false:true;presentation=options && options.presentation || "auto";
        screen=Quickshell.screens.find(s=>s.name===outputName) || Quickshell.screens[0];
        shell.claimPopup("folder-stack");visible=true;reloadTimer.restart();
    }
    function reload(){++generation;worker.running=false;rows=[];selected=0;error="";reloadTimer.restart();}
    function setOptions(key,value){
        if(key==="sort")sort=value;else if(key==="descending")descending=value;else presentation=value;
        shell.updateFolderStack(folder,{sort:sort,descending:descending,presentation:presentation});
        if(key!=="presentation")reload();
    }
    function itemIcon(item){const name=item.directory?"folder":item.kind.startsWith("image/")?"image-x-generic":item.kind.startsWith("video/")?"video-x-generic":"text-x-generic";return Quickshell.hasThemeIcon(name)?Quickshell.iconPath(name):Qt.resolvedUrl(item.directory?"icons/folder.svg":"icons/application.svg");}
    function openItem(){if(!rows[selected])return;const item=rows[selected];visible=false;if(item.directory)shell.openFiles(item.path);else AppLaunch.run(["xdg-open",item.uri]);}
    function moveSelection(delta){selected=Math.max(0,Math.min(rows.length-1,selected+delta));}
    onVisibleChanged:if(!visible){++generation;worker.running=false;reloadTimer.stop();if(shell.popupOwner==="folder-stack")shell.claimPopup("");}
    Connections {target:shell;function onDismissPopups(owner){if(owner!=="folder-stack")root.visible=false;}}
    MouseArea {anchors.fill:parent;onClicked:root.visible=false}
    Item {anchors.fill:parent;focus:true
        Keys.onEscapePressed:root.visible=false
        Keys.onUpPressed:root.moveSelection(root.grid?-Math.max(1,Math.floor(gridItems.width/gridItems.cellWidth)):-1)
        Keys.onDownPressed:root.moveSelection(root.grid?Math.max(1,Math.floor(gridItems.width/gridItems.cellWidth)):1)
        Keys.onLeftPressed:if(root.grid)root.moveSelection(-1)
        Keys.onRightPressed:if(root.grid)root.moveSelection(1)
        Keys.onReturnPressed:root.openItem()
    }
    PanelSurface {
        x:Math.max(8,parent.width-width-32);y:Math.max(30,parent.height-height-96)
        width:Math.min(root.grid?510:390,parent.width-16)
        height:Math.min(parent.height-136,Math.max(220,150+(root.grid?Math.ceil(root.rows.length/4)*96:root.rows.length*32)))
        MouseArea {anchors.fill:parent}
        Column {anchors.fill:parent;anchors.margins:14;spacing:6
            PanelLabel {width:parent.width;text:root.folderName+(root.total?" · "+root.total+(root.limited?"+":""):"");elide:Text.ElideMiddle;font.weight:Font.DemiBold;height:24}
            Row {width:parent.width;spacing:6
                DesktopComboBox {width:(parent.width-12)*0.42;model:["Name","Date Modified","Size","Kind"];currentIndex:["name","modified","size","type"].indexOf(root.sort);Accessible.name:"Sort folder stack";onActivated:index=>root.setOptions("sort",["name","modified","size","type"][index])}
                DesktopComboBox {width:(parent.width-12)*0.36;model:["Automatic","List","Grid"];currentIndex:["auto","list","grid"].indexOf(root.presentation);Accessible.name:"Folder stack presentation";onActivated:index=>root.setOptions("presentation",["auto","list","grid"][index])}
                ControlRow {width:(parent.width-12)*0.22;height:30;text:root.descending?"↓":"↑";Accessible.name:root.descending?"Sort ascending":"Sort descending";onClicked:root.setOptions("descending",!root.descending)}
            }
            Item {width:parent.width;height:Math.max(0,parent.height-104)
                ListView {id:items;anchors.fill:parent;visible:!root.grid;clip:true;model:root.rows;currentIndex:root.selected;onCurrentIndexChanged:positionViewAtIndex(currentIndex,ListView.Contain)
                    delegate:ControlRow {required property var modelData;required property int index;width:items.width;height:32;leftPadding:38;text:modelData.name
                        Image {x:5;y:5;width:22;height:22;asynchronous:true;sourceSize:Qt.size(44,44);fillMode:Image.PreserveAspectFit;source:root.itemIcon(modelData)}
                        selected:root.selected===index;onClicked:{root.selected=index;root.openItem();}
                    }
                    ScrollBar.vertical:DesktopScrollBar {}
                }
                GridView {id:gridItems;anchors.fill:parent;visible:root.grid;clip:true;cellWidth:Math.max(86,width/Math.max(1,Math.floor(width/100)));cellHeight:96;model:root.rows;currentIndex:root.selected;onCurrentIndexChanged:positionViewAtIndex(currentIndex,GridView.Contain)
                    delegate:AbstractButton {required property var modelData;required property int index;width:gridItems.cellWidth;height:94;Accessible.name:modelData.name;Accessible.selected:root.selected===index
                        background:Rectangle {radius:8;color:root.selected===parent.index?Theme.selection:"transparent"}
                        contentItem:Item {
                            Image {anchors.horizontalCenter:parent.horizontalCenter;y:4;width:48;height:48;sourceSize:Qt.size(96,96);asynchronous:true;fillMode:Image.PreserveAspectFit;source:root.itemIcon(parent.parent.modelData)}
                            PanelLabel {x:4;y:57;width:parent.width-8;height:32;maximumLineCount:2;wrapMode:Text.Wrap;elide:Text.ElideRight;horizontalAlignment:Text.AlignHCenter;font.pixelSize:11;text:parent.parent.modelData.name}
                        }
                        onClicked:{root.selected=index;root.openItem();}
                    }
                    ScrollBar.vertical:DesktopScrollBar {}
                }
                PanelLabel {visible:root.rows.length===0;width:parent.width;text:root.error || (worker.running || reloadTimer.running?"Loading…":"This folder is empty");wrapMode:Text.WordWrap;color:Theme.colors.muted;font.pixelSize:12}
            }
            ControlRow {width:parent.width;height:28;text:root.limited?"Show All in Files…":"Open in Files";onClicked:{root.visible=false;root.shell.openFiles(root.folder);}}
        }
    }
    Timer {id:reloadTimer;interval:1;onTriggered:if(root.visible && !worker.running){worker.requestGeneration=root.generation;worker.request={id:root.generation,action:"column-list",path:root.folder,sort:root.sort==="type"?"kind":root.sort,descending:root.descending};worker.running=true;}else if(root.visible)restart()}
    Process {id:worker;stdinEnabled:true;property int requestGeneration:-1;property var request:({});command:[Quickshell.env("GLOBAL_MENU_PYTHON") || "python3",Qt.resolvedUrl("files.py").toString().replace("file://","")]
        onStarted:write(JSON.stringify(request)+"\n")
        stdout:SplitParser {onRead:line=>{if(worker.requestGeneration!==root.generation || !root.visible)return;try{const result=JSON.parse(line);if(result.id!==root.generation)return;root.total=(result.rows || []).length;root.limited=!!result.limited;root.rows=result.rows || [];root.error=result.error || "";}catch(e){root.error="This folder is unavailable";}worker.running=false;}}
    }
}

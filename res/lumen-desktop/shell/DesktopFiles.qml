import QtQuick
import QtQuick.Controls
import Quickshell
import Quickshell.Io
import "DesktopLayoutModel.js" as Layout
import "DesktopDragModel.js" as DragModel
import "FileDropModel.js" as FileDrop

// An opt-in owned surface. The surrounding layer chooses its output and mask.
Item {
    id:root
    required property var shell
    property bool active:false
    DesktopLayoutState {id:arrangement;directory:root.directory;rows:root.service.rows;listingBusy:root.service.busy===true;active:root.active}
    readonly property var displayItems:Layout.build(active?service.rows:[],arrangement.layout,arrangement.expanded,stage.width,stage.height,Date.now())
    readonly property bool externalDragActive:active && shell.nativeFileDragActive===true
    readonly property var inputRectangles:externalDragActive || (shell.nativeFileDragAvailable===true && backgroundDrop.containsDrag && dragTail.running) || dragSnapshot.length>0 || nativeDragActive || dropConfirmation.visible || trashConfirmation.visible || editor.visible || actions.visible?[{x:stage.x,y:stage.y,width:stage.width,height:stage.height}]:displayItems.filter(item=>!item.overflow).map(item=>({x:stage.x+item.x,y:stage.y+item.y,width:item.width,height:item.height}))
    // Native drop completion can precede Qt delivery. Keep an entered target
    // until Qt consumes that drop, with a short bound and no disconnected lease.
    onExternalDragActiveChanged:if(externalDragActive)dragTail.stop();else if(backgroundDrop.containsDrag)dragTail.restart()
    Timer {id:dragTail;interval:200}
    readonly property int overflowCount:displayItems.filter(item=>item.overflow).length
    readonly property var desktopLayout:arrangement.layout
    function grouping(mode){selectedPaths=[];currentPath="";anchorPath="";arrangement.group(mode);}
    function desktopActions(){selectedPaths=[];currentPath="";anchorPath="";actions.open();focusContents();}

    property bool resolveDirectory:true
    property string directory:""
    property string sort:"name"
    property bool descending:false
    property var service:internalFiles
    property string error:""
    property var selectedPaths:[]
    property string currentPath:""
    property string anchorPath:""
    property var pendingTrash:[]
    property string pendingRename:""
    property bool nativeDragActive:false
    property string nativeUris:""
    property string nativeToken:""
    property point dragAnchor:Qt.point(0,0)
    property var pendingDropPaths:[]
    property string pendingDropDestination:""
    function beginNativeDrag(){
        if(!dragSnapshot.length || dragSnapshot[0].stack || nativeDragActive)return;
        try {nativeUris=FileDrop.uris(displayItems.filter(item=>dragSnapshot.some(selected=>selected.key===item.key)));nativeToken=Date.now()+":"+Math.random();nativeDragActive=true;}
        catch(error){root.error=String(error);}
    }
    function receiveInternalDrop(x,y){
        if(!nativeDragActive || arrangement.layout.grouping!=="none")return false;
        const placement=DragModel.translate(dragSnapshot,x-dragAnchor.x,y-dragAnchor.y,stage.width,stage.height);
        dragSnapshot=[];dragPreview=[];if(placement.length)arrangement.place(placement,stage.width,stage.height);return true;
    }
    function endNativeDrag(){nativeDragActive=false;nativeUris="";nativeToken="";cancelDrag();}
    function proposeCopy(urls,destination){
        dropConfirmation.close();pendingDropPaths=[];pendingDropDestination="";
        try {const paths=FileDrop.paths(urls);if(typeof destination!=="string" || !destination.startsWith("/") || /[\u0000-\u001f\u007f]/.test(destination))throw Error("The destination folder is invalid.");root.error="";pendingDropPaths=paths;pendingDropDestination=destination;dropConfirmation.open();focusContents();Qt.callLater(()=>cancelDrop.forceActiveFocus());return true;}
        catch(error){root.error=String(error);return false;}
    }
    function confirmDrop(){const paths=pendingDropPaths.slice(),destination=pendingDropDestination;pendingDropPaths=[];pendingDropDestination="";dropConfirmation.close();if(paths.length)service.startJob("copy",paths,destination,"skip");}
    property var dragSnapshot:[]
    property var dragPreview:[]
    function startDrag(key,anchor){dragProxy.x=0;dragProxy.y=0;dragAnchor=anchor || Qt.point(0,0);dragSnapshot=DragModel.begin(displayItems,key,selectedPaths,arrangement.layout.grouping);if(!dragSnapshot.length && arrangement.layout.grouping!=="none")dragSnapshot=displayItems.filter(item=>!item.stack && !item.overflow && (selectedPaths.includes(item.row.path) || item.key===key)).map(item=>({key:item.key,x:item.x,y:item.y,width:item.width,height:item.height,stack:false}));dragPreview=[];}
    function previewDrag(){if(arrangement.layout.grouping!=="none" && dragSnapshot.length && !dragSnapshot[0].stack)return;dragPreview=DragModel.translate(dragSnapshot,dragProxy.x,dragProxy.y,stage.width,stage.height);}
    function finishDrag(){const placement=dragPreview;dragSnapshot=[];dragPreview=[];if(placement.length)arrangement.place(placement,stage.width,stage.height);}
    function cancelDrag(){dragSnapshot=[];dragPreview=[];}
    property bool keyboardActive:false
    readonly property var current:service.rows.find(row=>row.path===currentPath) || null
    readonly property alias inputArea:stage
    readonly property bool dropConfirmationVisible:dropConfirmation.visible
    readonly property bool confirmationVisible:trashConfirmation.visible
    signal keyboardRequested(bool requested)

    function focusContents(){keyboardActive=true;keyboardRequested(true);stage.forceActiveFocus();}
    function leaveKeyboard(){keyboardActive=false;keyboardRequested(false);}
    function choose(index,modifiers){
        const row=service.rows[index];if(!row)return;
        const primary=(modifiers || 0)&(Qt.ControlModifier|Qt.MetaModifier);
        const anchor=service.rows.findIndex(item=>item.path===anchorPath);
        if(((modifiers || 0)&Qt.ShiftModifier) && anchor>=0)selectedPaths=service.rows.slice(Math.min(anchor,index),Math.max(anchor,index)+1).map(item=>item.path);
        else if(primary)selectedPaths=selectedPaths.includes(row.path)?selectedPaths.filter(path=>path!==row.path):selectedPaths.concat([row.path]);
        else selectedPaths=[row.path];
        if(!((modifiers || 0)&Qt.ShiftModifier))anchorPath=row.path;
        currentPath=row.path;focusContents();
    }
    function step(delta,modifiers){const index=service.rows.findIndex(row=>row.path===currentPath);choose(index<0?(delta<0?service.rows.length-1:0):Math.max(0,Math.min(service.rows.length-1,index+delta)),modifiers || 0);}
    function quickLookStep(delta){step(delta);if(current)shell.quickLookWindow.show(current.path,root);}
    function openCurrent(){if(!current)return;if(current.directory)shell.openFiles(current.path);else service.act("open",{path:current.path});leaveKeyboard();}
    function askTrash(){
        pendingTrash=selectedPaths.filter(path=>service.rows.some(row=>row.path===path));
        if(!pendingTrash.length)return;
        trashConfirmation.open();focusContents();Qt.callLater(()=>cancelTrash.forceActiveFocus());
    }
    function confirmTrash(){
        const paths=pendingTrash.slice();pendingTrash=[];trashConfirmation.close();
        if(paths.length)service.startJob("trash",paths);
    }
    function beginRename(){if(selectedPaths.length!==1 || !current)return;pendingRename=current.path;nameField.text=current.name;editor.mode="rename";editor.open();focusContents();Qt.callLater(()=>{nameField.forceActiveFocus();nameField.selectAll();});}
    function beginFolder(){pendingRename="";nameField.text="";editor.mode="mkdir";editor.open();focusContents();Qt.callLater(()=>nameField.forceActiveFocus());}
    function submitName(){if(!nameField.text)return;service.act(editor.mode,{path:editor.mode==="rename"?pendingRename:directory,name:nameField.text});editor.close();}
    function context(){if(!current)return;actions.open();focusContents();}
    function activate(){if(!active)return;if(resolveDirectory){if(!location.running)location.running=true;}else if(directory && service===internalFiles)service.navigate(directory);}
    onActiveChanged:{if(active)activate();else {endNativeDrag();dropConfirmation.close();selectedPaths=[];currentPath="";anchorPath="";trashConfirmation.close();editor.close();actions.close();leaveKeyboard();}}
    Component.onCompleted:if(active)activate()
    onSortChanged:if(active && service===internalFiles)service.refresh()
    onDescendingChanged:if(active && service===internalFiles)service.refresh()
    FilesState {id:internalFiles;persistViewPreferences:false;sort:root.sort;descending:root.descending;path:root.directory;active:root.active && root.service===internalFiles && !!root.directory}
    Connections {target:root.service
        function onRowsChanged(){root.selectedPaths=root.selectedPaths.filter(path=>root.service.rows.some(row=>row.path===path));if(!root.service.rows.some(row=>row.path===root.currentPath))root.currentPath=root.selectedPaths[0] || "";if(!root.service.rows.some(row=>row.path===root.anchorPath))root.anchorPath=root.currentPath;}
        function onInformationReady(details){info.show(details);}
    }
    FileInfoWindow {id:info}
    Process {
        id:location
        command:["lumen-desktop-files"]
        stdout:StdioCollector {onStreamFinished:{
            try {const data=JSON.parse(text);if(data.error)throw new Error(data.error);root.directory=data.disabled?"":data.path;root.error=data.disabled?"Desktop files are disabled in your user-folder configuration.":data.exists?"":"The Desktop folder is unavailable. Open Files to create or select it.";if(data.exists && root.active && root.service===internalFiles)root.service.navigate(data.path);}
            catch(error){root.error="Could not locate the Desktop folder: "+error;}
        }}
    }
    Item {
        id:stage
        anchors {top:parent.top;right:parent.right;bottom:parent.bottom;topMargin:Theme.barHeight+16;left:parent.left;leftMargin:Theme.preferences.dockPosition==="left"?(Theme.preferences.dockSize || 48)+100:16;rightMargin:Theme.preferences.dockPosition==="right"?Math.min(parent.width-110,(Theme.preferences.dockSize || 48)+100):16;bottomMargin:110}

        focus:root.keyboardActive
        Keys.onPressed:event=>{
            const primary=event.modifiers&(Qt.ControlModifier|Qt.MetaModifier);
            if(event.key===Qt.Key_Escape){if(root.nativeDragActive)dragProxy.Drag.cancel();else root.cancelDrag();dropConfirmation.close();actions.close();editor.close();trashConfirmation.close();root.pendingTrash=[];root.leaveKeyboard();}
            else if(primary && event.key===Qt.Key_A){root.selectedPaths=root.service.rows.map(row=>row.path);if(root.selectedPaths.length)root.currentPath=root.selectedPaths[0];}
            else if(primary && event.key===Qt.Key_C)root.service.stage(root.selectedPaths,"copy");
            else if(primary && event.key===Qt.Key_X)root.service.stage(root.selectedPaths,"move");
            else if(primary && event.key===Qt.Key_V)root.service.paste();
            else if(primary && event.key===Qt.Key_Z)root.service.undo();
            else if(event.key===Qt.Key_Delete || primary && event.key===Qt.Key_Backspace)root.askTrash();
            else if(event.key===Qt.Key_F2)root.beginRename();
            else if(event.key===Qt.Key_Return || event.key===Qt.Key_Enter)root.openCurrent();
            else if(event.key===Qt.Key_Space && root.current)root.shell.quickLookWindow.show(root.current.path,root);
            else if(event.key===Qt.Key_Menu || event.key===Qt.Key_F10 && (event.modifiers&Qt.ShiftModifier))root.context();
            else if(event.key===Qt.Key_Down)root.step(1,event.modifiers);
            else if(event.key===Qt.Key_Up)root.step(-1,event.modifiers);
            else if(event.key===Qt.Key_Right)root.step(Math.max(1,Math.floor(grid.height/grid.cellHeight)),event.modifiers);
            else if(event.key===Qt.Key_Left)root.step(-Math.max(1,Math.floor(grid.height/grid.cellHeight)),event.modifiers);
            else {event.accepted=false;return;}
            event.accepted=true;
        }
        Item {id:dragProxy
            Drag.active:root.nativeDragActive
            Drag.dragType:Drag.Automatic
            Drag.source:dragProxy
            Drag.supportedActions:Qt.CopyAction
            Drag.proposedAction:Qt.CopyAction
            Drag.mimeData:({"text/uri-list":root.nativeUris,"application/x-lumen-desktop-arrangement":root.nativeToken})
            Drag.onDragFinished:dropAction=>root.endNativeDrag()
        }
        Item {
            id:grid;anchors.fill:parent
        DropArea {anchors.fill:parent
            id:backgroundDrop
            objectName:"desktopBackgroundDrop"
            onEntered:drag=>{drag.accepted=(root.nativeDragActive && drag.formats.includes("application/x-lumen-desktop-arrangement")) || (!root.nativeDragActive && drag.hasUrls && (drag.supportedActions & Qt.CopyAction));}
            onDropped:drop=>{if(root.nativeDragActive && drop.getDataAsString("application/x-lumen-desktop-arrangement")===root.nativeToken && root.receiveInternalDrop(drop.x,drop.y))drop.accept(Qt.CopyAction);else if(!root.nativeDragActive && drop.hasUrls && (drop.supportedActions & Qt.CopyAction) && root.proposeCopy(drop.urls.map(url=>url.toString()),root.directory))drop.accept(Qt.CopyAction);}
        }
            Repeater {model:root.displayItems
                AbstractButton {
                    id:tile
                    required property var modelData
                    required property int index
                    readonly property var fileRow:modelData.row || ({path:"",name:modelData.label,directory:false})
                    readonly property var draggedPoint:root.dragPreview.find(item=>item.key===modelData.key) || null
                    x:draggedPoint?draggedPoint.x:modelData.x;y:draggedPoint?draggedPoint.y:modelData.y;width:modelData.width;height:modelData.height;visible:!modelData.overflow
                    Accessible.name:modelData.stack?modelData.label+" stack, "+modelData.children.length+" items":fileRow.name
                    onClicked:{if(modelData.stack){root.focusContents();arrangement.toggle(modelData.key);}else root.choose(modelData.sourceIndex,0);}
                    Keys.onPressed:event=>{if(modelData.stack && [Qt.Key_Return,Qt.Key_Enter,Qt.Key_Space].includes(event.key)){arrangement.toggle(modelData.key);event.accepted=true;}else event.accepted=false;}
                    Accessible.role:Accessible.ListItem;Accessible.selected:!modelData.stack && root.selectedPaths.includes(fileRow.path)
                    background:Rectangle {radius:7;color:root.selectedPaths.includes(tile.fileRow.path)?Theme.colors.accent:tile.hovered?"#30303030":"transparent";border.color:tile.activeFocus?Theme.colors.text:"transparent"}
                    contentItem:Item {
                        SystemGlyph {anchors.horizontalCenter:parent.horizontalCenter;y:8;width:46;height:46;name:tile.modelData.stack || tile.fileRow.directory?"folder":"document";ink:root.selectedPaths.includes(tile.fileRow.path)?Theme.colors.selectedText:Theme.colors.accent}
                        PanelLabel {x:5;y:61;width:98;height:34;text:tile.modelData.stack?tile.modelData.label+" ("+tile.modelData.children.length+")":(tile.fileRow.link?"↗ ":"")+tile.fileRow.name;textFormat:Text.PlainText;wrapMode:Text.Wrap;maximumLineCount:2;elide:Text.ElideRight;horizontalAlignment:Text.AlignHCenter;font.pixelSize:11;color:root.selectedPaths.includes(tile.fileRow.path)?Theme.colors.selectedText:"white";style:Text.Outline;styleColor:"#66000000"}
                    }
                    DropArea {anchors.fill:parent;enabled:tile.fileRow.directory && !tile.modelData.stack
                        onEntered:drag=>{drag.accepted=!root.nativeDragActive && drag.hasUrls && (drag.supportedActions & Qt.CopyAction);}
                        onDropped:drop=>{if(!root.nativeDragActive && drop.hasUrls && (drop.supportedActions & Qt.CopyAction) && root.proposeCopy(drop.urls.map(url=>url.toString()),tile.fileRow.path))drop.accept(Qt.CopyAction);}
                    }
                    MouseArea {
                        anchors.fill:parent;acceptedButtons:Qt.LeftButton|Qt.RightButton
                        drag.target:pressedButtons & Qt.LeftButton?dragProxy:null
                        drag.threshold:8
                        property bool wasDragged:false
                        onPositionChanged:if(drag.active){wasDragged=true;root.previewDrag();}
                        drag.onActiveChanged:if(drag.active){wasDragged=true;root.previewDrag();root.beginNativeDrag();}
                        onPressed:event=>{
                            wasDragged=false;
                            if(!tile.modelData.stack){
                                if(!root.selectedPaths.includes(tile.fileRow.path) || event.modifiers)root.choose(tile.modelData.sourceIndex,event.modifiers);
                                else {root.currentPath=tile.fileRow.path;root.focusContents();}
                            }
                            root.startDrag(tile.modelData.key,mapToItem(stage,event.x,event.y));
                        }
                        onReleased:event=>{if(!root.nativeDragActive){if(wasDragged)root.finishDrag();else root.cancelDrag();}}
                        onCanceled:if(!root.nativeDragActive)root.cancelDrag()
                        onClicked:event=>{if(wasDragged)return;if(tile.modelData.stack){if(event.button===Qt.RightButton)root.desktopActions();else {root.focusContents();arrangement.toggle(tile.modelData.key);}}else if(event.button===Qt.RightButton){root.currentPath=tile.fileRow.path;root.context();}}
                        onDoubleClicked:event=>{if(!tile.modelData.stack && event.button===Qt.LeftButton)root.openCurrent();}
                    }
                }
            }
        }
        PanelLabel {anchors.bottom:parent.bottom;width:parent.width;wrapMode:Text.Wrap;text:root.error || arrangement.error || root.service.error || root.service.warning || (root.overflowCount?root.overflowCount+" desktop items do not fit. Open Desktop in Files to access every item.":"");textFormat:Text.PlainText;color:"white";style:Text.Outline;styleColor:"black";visible:!!text}
        Popup {id:actions;width:Math.min(240,stage.width);height:Math.min(460,stage.height);x:Math.max(0,stage.width-width);y:0;modal:true;focus:true;padding:8
            background:Rectangle {radius:10;color:Theme.colors.background;border.color:Theme.outline}
            contentItem:Column {spacing:2
                ControlRow {width:parent.width;text:"Open";enabled:!!root.current;onClicked:{actions.close();root.openCurrent();}}
                ControlRow {width:parent.width;text:"Open Desktop in Files";onClicked:{actions.close();root.shell.openFiles(root.directory);root.leaveKeyboard();}}
                ControlRow {width:parent.width;text:"New Folder…";enabled:root.service.writable;onClicked:{actions.close();root.beginFolder();}}
                ControlRow {width:parent.width;text:"Rename…";enabled:root.selectedPaths.length===1;onClicked:{actions.close();root.beginRename();}}
                ControlRow {width:parent.width;text:"Get Info";enabled:!!root.current;onClicked:{actions.close();root.service.send("info",{path:root.current.path});}}
                ControlRow {width:parent.width;text:"Copy";enabled:root.selectedPaths.length>0;onClicked:{actions.close();root.service.stage(root.selectedPaths,"copy");}}
                ControlRow {width:parent.width;text:"Move to Trash…";enabled:root.selectedPaths.length>0;onClicked:{actions.close();root.askTrash();}}
                ControlRow {width:parent.width;text:"Reset Icon Arrangement";onClicked:{actions.close();arrangement.request("reset");}}
                ControlRow {width:parent.width;text:"No Stacks";onClicked:{actions.close();root.grouping("none");}}
                ControlRow {width:parent.width;text:"Group by Kind";onClicked:{actions.close();root.grouping("kind");}}
                ControlRow {width:parent.width;text:"Group by Modification Date";onClicked:{actions.close();root.grouping("modified");}}
                ControlRow {width:parent.width;text:"Group by Tags";onClicked:{actions.close();root.grouping("tags");}}
                ControlRow {width:parent.width;text:"Refresh";onClicked:{actions.close();root.service.refresh();}}
            }
        }
        Popup {id:dropConfirmation;width:Math.min(360,stage.width);height:Math.min(dropBody.implicitHeight+28,stage.height);x:stage.width-width;y:Math.max(0,(stage.height-height)/2);modal:true;focus:true;padding:14
            onClosed:{root.pendingDropPaths=[];root.pendingDropDestination="";}
            background:Rectangle {radius:12;color:Theme.colors.background;border.color:Theme.outline}
            contentItem:Column {id:dropBody;spacing:12
                PanelLabel {text:"Copy Dropped Files?";font.weight:Font.DemiBold}
                PanelLabel {width:parent.width;wrapMode:Text.Wrap;text:root.pendingDropPaths.length+" item(s) to "+root.pendingDropDestination;maximumLineCount:3;elide:Text.ElideMiddle;textFormat:Text.PlainText}
                PanelLabel {width:parent.width;wrapMode:Text.Wrap;text:"Existing names will be skipped. Originals will remain in place.";textFormat:Text.PlainText}
                DialogButton {id:cancelDrop;width:parent.width;text:"Cancel";defaultAction:true;onClicked:dropConfirmation.close()}
                DialogButton {width:parent.width;text:"Copy Files";primary:true;onClicked:root.confirmDrop()}
            }
        }
        Popup {id:trashConfirmation;width:Math.min(330,stage.width);height:Math.min(trashBody.implicitHeight+28,stage.height);x:stage.width-width;y:Math.max(0,(stage.height-height)/2);modal:true;focus:true;padding:14
            onClosed:root.pendingTrash=[]
            background:Rectangle {radius:12;color:Theme.colors.background;border.color:Theme.outline}
            contentItem:Column {id:trashBody;spacing:12
                PanelLabel {text:"Move to Trash?";font.weight:Font.DemiBold}
                PanelLabel {width:parent.width;wrapMode:Text.Wrap;text:root.pendingTrash.length+" selected item(s) will be moved to Trash. You can put them back from Files.";textFormat:Text.PlainText}
                DialogButton {id:cancelTrash;width:parent.width;text:"Cancel";defaultAction:true;onClicked:trashConfirmation.close()}
                DialogButton {width:parent.width;text:"Move to Trash";destructive:true;onClicked:root.confirmTrash()}
            }
        }
        Popup {id:editor;property string mode:"mkdir";width:Math.min(330,stage.width);height:170;x:stage.width-width;y:Math.max(0,(stage.height-height)/2);modal:true;focus:true;padding:14
            background:Rectangle {radius:12;color:Theme.colors.background;border.color:Theme.outline}
            contentItem:Column {spacing:12
                PanelLabel {text:editor.mode==="rename"?"Rename":"New Folder";font.weight:Font.DemiBold}
                TextField {id:nameField;width:parent.width;Accessible.name:"Name";color:Theme.colors.text;selectByMouse:true;background:Rectangle {radius:6;color:Theme.colors.field;border.color:Theme.colors.accent}onAccepted:if(text)root.submitName()}
                Row {width:parent.width;spacing:8
                    DialogButton {width:(parent.width-8)/2;text:"Cancel";onClicked:editor.close()}
                    DialogButton {width:(parent.width-8)/2;text:editor.mode==="rename"?"Rename":"Create";enabled:!!nameField.text;primary:true;onClicked:root.submitName()}
                }
            }
        }
    }
}

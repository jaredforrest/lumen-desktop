import QtQuick
import QtQuick.Controls
import Quickshell
import Quickshell.Io

// An opt-in owned surface. The surrounding layer chooses its output and mask.
Item {
    id:root
    required property var shell
    property bool active:false
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
    property bool keyboardActive:false
    readonly property var current:service.rows.find(row=>row.path===currentPath) || null
    readonly property alias inputArea:stage
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
        currentPath=row.path;grid.positionViewAtIndex(index,GridView.Contain);focusContents();
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
    onActiveChanged:{if(active)activate();else {selectedPaths=[];currentPath="";anchorPath="";trashConfirmation.close();editor.close();actions.close();leaveKeyboard();}}
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
        command:[Quickshell.env("GLOBAL_MENU_PYTHON") || "python3",Qt.resolvedUrl("desktop_files.py").toString().replace(/^file:\/\//,"")]
        stdout:StdioCollector {onStreamFinished:{
            try {const data=JSON.parse(text);if(data.error)throw new Error(data.error);root.directory=data.disabled?"":data.path;root.error=data.disabled?"Desktop files are disabled in your user-folder configuration.":data.exists?"":"The Desktop folder is unavailable. Open Files to create or select it.";if(data.exists && root.active && root.service===internalFiles)root.service.navigate(data.path);}
            catch(error){root.error="Could not locate the Desktop folder: "+error;}
        }}
    }
    Item {
        id:stage
        anchors {top:parent.top;right:parent.right;bottom:parent.bottom;topMargin:Theme.barHeight+16;rightMargin:Theme.preferences.dockPosition==="right"?Math.min(parent.width-110,(Theme.preferences.dockSize || 48)+100):16;bottomMargin:110}
        width:Math.max(110,Math.min(432,parent.width*0.46))
        focus:root.keyboardActive
        Keys.onPressed:event=>{
            const primary=event.modifiers&(Qt.ControlModifier|Qt.MetaModifier);
            if(event.key===Qt.Key_Escape){actions.close();editor.close();trashConfirmation.close();root.pendingTrash=[];root.leaveKeyboard();}
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
        GridView {
            id:grid;anchors.fill:parent;clip:true;cellWidth:108;cellHeight:100;flow:GridView.FlowTopToBottom;layoutDirection:Qt.RightToLeft
            model:root.active?root.service.rows:[]
            ScrollBar.horizontal:DesktopScrollBar {}
            delegate:AbstractButton {
                required property var modelData
                required property int index
                width:108;height:100
                Accessible.name:modelData.name;Accessible.role:Accessible.ListItem;Accessible.selected:root.selectedPaths.includes(modelData.path)
                background:Rectangle {radius:7;color:root.selectedPaths.includes(parent.modelData.path)?Theme.colors.accent:parent.hovered?"#30303030":"transparent";border.color:parent.activeFocus?Theme.colors.text:"transparent"}
                contentItem:Item {
                    SystemGlyph {anchors.horizontalCenter:parent.horizontalCenter;y:8;width:46;height:46;name:modelData.directory?"folder":"document";ink:root.selectedPaths.includes(modelData.path)?Theme.colors.selectedText:Theme.colors.accent}
                    PanelLabel {x:5;y:61;width:98;height:34;text:(modelData.link?"↗ ":"")+modelData.name;textFormat:Text.PlainText;wrapMode:Text.Wrap;maximumLineCount:2;elide:Text.ElideRight;horizontalAlignment:Text.AlignHCenter;font.pixelSize:11;color:root.selectedPaths.includes(modelData.path)?Theme.colors.selectedText:"white";style:Text.Outline;styleColor:"#66000000"}
                }
                onClicked:root.choose(index,0)
                onDoubleClicked:root.openCurrent()
                MouseArea {anchors.fill:parent;acceptedButtons:Qt.LeftButton|Qt.RightButton;onClicked:event=>{if(event.button!==Qt.RightButton || !root.selectedPaths.includes(modelData.path))root.choose(index,event.modifiers);if(event.button===Qt.RightButton){root.currentPath=modelData.path;root.context();}}onDoubleClicked:event=>{if(event.button===Qt.LeftButton)root.openCurrent();}}
            }
        }
        PanelLabel {anchors.bottom:parent.bottom;width:parent.width;wrapMode:Text.Wrap;text:root.error || root.service.error || root.service.warning || "";textFormat:Text.PlainText;color:"white";style:Text.Outline;styleColor:"black";visible:!!text}
        Popup {id:actions;width:Math.min(240,stage.width);height:320;x:stage.width-width;y:0;modal:true;focus:true;padding:8
            background:Rectangle {radius:10;color:Theme.colors.background;border.color:Theme.outline}
            contentItem:Column {spacing:2
                ControlRow {width:parent.width;text:"Open";enabled:!!root.current;onClicked:{actions.close();root.openCurrent();}}
                ControlRow {width:parent.width;text:"Open Desktop in Files";onClicked:{actions.close();root.shell.openFiles(root.directory);root.leaveKeyboard();}}
                ControlRow {width:parent.width;text:"New Folder…";enabled:root.service.writable;onClicked:{actions.close();root.beginFolder();}}
                ControlRow {width:parent.width;text:"Rename…";enabled:root.selectedPaths.length===1;onClicked:{actions.close();root.beginRename();}}
                ControlRow {width:parent.width;text:"Get Info";enabled:!!root.current;onClicked:{actions.close();root.service.send("info",{path:root.current.path});}}
                ControlRow {width:parent.width;text:"Copy";enabled:root.selectedPaths.length>0;onClicked:{actions.close();root.service.stage(root.selectedPaths,"copy");}}
                ControlRow {width:parent.width;text:"Move to Trash…";enabled:root.selectedPaths.length>0;onClicked:{actions.close();root.askTrash();}}
                ControlRow {width:parent.width;text:"Refresh";onClicked:{actions.close();root.service.refresh();}}
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

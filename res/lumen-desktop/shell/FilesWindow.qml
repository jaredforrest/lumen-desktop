import QtQuick
import QtQuick.Controls
import QtQuick.Window
import QtQuick.Effects
import Quickshell

FloatingWindow {
    id:root
    readonly property bool selectionActive:content.Window.window?content.Window.window.active:false
    readonly property color selectionFill:selectionActive?Theme.colors.accent:Theme.selection
    readonly property color selectionText:selectionActive?Theme.colors.selectedText:Theme.colors.text
    required property var shell
    required property var service
    property int windowNumber:1
    title:"Files"+(windowNumber>1?" "+windowNumber:"")+" — "+(service.path==="trash:"?"Trash":service.path.split("/").pop() || "Computer")
    visible:false
    implicitWidth:900;implicitHeight:600
    minimumSize:Qt.size(480,320)
    color:"transparent"
    surfaceFormat.opaque:false
    property bool minimized:false
    property var swayContainer:0
    readonly property bool listMode:service.viewMode==="list"
    readonly property bool galleryMode:service.viewMode==="gallery"
    readonly property bool columnsMode:service.viewMode==="columns"
    property int selected:-1
    property var selectedPaths:[]
    property int selectionAnchor:-1
    property var tabs:[{path:Quickshell.env("HOME"),history:[],historyIndex:-1}]
    property int activeTab:0
    readonly property int tabHeight:tabs.length>1?30:0
    property var applications:[]
    property bool dragging:false
    property var information:({})
    FileInfoWindow {id:infoWindow}
    property var dragUris:[]
    function beginDrag(){if(dragUris.length)dragging=true;}
    function newTab(){saveTab();tabs=tabs.concat([{path:service.path,history:[],historyIndex:-1}]);switchTab(tabs.length-1);}
    function saveTab(){const next=tabs.slice();next[activeTab]={path:service.path,history:service.history.slice(),historyIndex:service.historyIndex};tabs=next;}
    function switchTab(index){if(index===activeTab)return;saveTab();activeTab=index;service.history=tabs[index].history || [];service.historyIndex=tabs[index].historyIndex===undefined?-1:tabs[index].historyIndex;selectedPaths=[];filter.text="";service.query="";service.navigate(tabs[index].path,service.historyIndex);}
    function closeTab(){if(tabs.length<2){visible=false;return;}const next=tabs.slice();next.splice(activeTab,1);tabs=next;activeTab=Math.min(activeTab,tabs.length-1);service.history=tabs[activeTab].history || [];service.historyIndex=tabs[activeTab].historyIndex;selectedPaths=[];navigate(tabs[activeTab].path);}
    function selectedUris(){return selectedPaths.map(p=>"file://"+p.split("/").map(encodeURIComponent).join("/"));}
    function isSelected(index){return service.rows[index] && selectedPaths.includes(service.rows[index].path);}
    function selectAll(){selectedPaths=service.rows.map(r=>r.path);selected=service.rows.length?0:-1;}
    onVisibleChanged:service.active=visible

    property bool previewOpen:false
    readonly property var current:service.rows[selected] || null
    readonly property int sidebarWidth:width<720?160:190
    function fitScreen() {
        if(!visible || !screen)return;
        const fitWidth=Math.max(480,Math.min(width,screen.width-32));
        const fitHeight=Math.max(320,Math.min(height,screen.height-150));
        implicitWidth=fitWidth;implicitHeight=fitHeight;
        if(swayContainer && Quickshell.env("SWAYSOCK"))Quickshell.execDetached(["swaymsg","[con_id="+swayContainer+"] resize set width "+Math.round(fitWidth)+" px height "+Math.round(fitHeight)+" px, move position center"]);

    }
    Timer {id:fitTimer;interval:100;onTriggered:root.fitScreen()}
    Connections {target:root.shell.exportedMenu;function onStateChanged(){const state=root.shell.exportedMenu.state;if(state.appName==="org.quickshell" && state.title===root.title)root.swayContainer=state.windowId;}}
    Connections {target:root.screen;function onWidthChanged(){fitTimer.restart();}function onHeightChanged(){fitTimer.restart();}}
    function openPath(path) {
        if(minimized && Quickshell.env("SWAYSOCK")){
            Quickshell.execDetached(["swaymsg",'[con_id='+swayContainer+'] move workspace current, focus']);minimized=false;
        }
        const wasVisible=visible;visible=true;
        filter.text="";service.query="";service.navigate(path || service.path);
        if(!wasVisible)fitTimer.restart();
        activationTimer.epoch=shell.focusEpoch;activationTimer.restart();
        Qt.callLater(()=>{if(content.Window.window){if(content.Window.window.visibility===Window.Minimized)content.Window.window.showNormal();content.Window.window.requestActivate();}content.forceActiveFocus();});
    }
    function dispatch(action) {
        if(content.Window.window)content.Window.window.requestActivate();
        content.forceActiveFocus();
        if(action==="open")openCurrent();
        else if(action==="copy" || action==="cut")stage(action==="cut"?"move":"copy");
        else if(action==="paste")service.paste();
        else if(action==="undo")service.undo();
        else if(action==="new-window")shell.newFilesWindow(service.path);
        else if(action==="new-tab")newTab();
        else if(["grid","list","columns","gallery"].includes(action))service.viewMode=action;
        else if(action==="info" && current)service.send("info",{path:current.path});
        else if(action==="preview")previewOpen=!previewOpen;
        else if(action==="hidden"){service.hidden=!service.hidden;service.refresh();}
        else if(action==="refresh")service.refresh();
        else if(action==="back" || action==="forward")service.back(action==="back"?-1:1);
        else if(action==="up")navigate(service.parentPath);
        else if(action==="home")navigate(Quickshell.env("HOME"));
        else dialogFor(action);
    }
    function navigate(path){filter.text="";service.query="";service.navigate(path);}
    function focusContents(){if(content.Window.window)content.Window.window.requestActivate();content.forceActiveFocus();activationTimer.epoch=shell.focusEpoch;activationTimer.restart();}
    Timer {id:activationTimer;property int epoch;interval:100;onTriggered:{if(epoch!==root.shell.focusEpoch)return;const window=root.shell.windows.find(w=>w.appId==="org.quickshell" && w.title===root.title);if(window)root.shell.focusWindow(window);}}
    function quickLookStep(delta){step(delta,false);if(current)shell.quickLookWindow.show(current.path,root);}
    function openCurrent(){if(!current || service.busy)return;if(current.directory)navigate(current.path);else service.act("open",{path:current.path});}
    function choose(index,modifiers){
        const row=service.rows[index];if(!row)return;
        if((modifiers || 0)&Qt.ShiftModifier){const from=selectionAnchor<0?index:selectionAnchor;selectedPaths=service.rows.slice(Math.min(from,index),Math.max(from,index)+1).map(r=>r.path);}
        else if((modifiers || 0)&Qt.ControlModifier){selectedPaths=selectedPaths.includes(row.path)?selectedPaths.filter(p=>p!==row.path):selectedPaths.concat([row.path]);selectionAnchor=index;}
        else {selectedPaths=[row.path];selectionAnchor=index;}
        selected=index;content.forceActiveFocus();
    }
    function step(delta,extend){choose(Math.max(0,Math.min(service.rows.length-1,selected+delta)),extend?Qt.ShiftModifier:0);if(listMode)list.positionViewAtIndex(selected,ListView.Contain);else if(galleryMode)galleryStrip.positionViewAtIndex(selected,ListView.Contain);else if(!columnsMode)grid.positionViewAtIndex(selected,GridView.Contain);}
    function stage(action){if(selectedPaths.length)service.stage(selectedPaths,action);}
    function dialogFor(action){
        if(service.busy)return;
        if(["rename","trash"].includes(action) && !current)return;
        editor.action=action;editor.target=current?current.path:"";editor.label=selectedPaths.length>1?selectedPaths.length+" selected items":current?current.name:"";
        entry.text=action==="rename"?current.name:action==="goto"?service.path:"";
        editor.open();Qt.callLater(()=>{if(action==="trash")cancelEditor.forceActiveFocus();else {entry.forceActiveFocus();entry.selectAll();}});
    }
    function submit(){
        const action=editor.action;editor.close();
        if(action==="goto")navigate(entry.text);
        else if(action==="mkdir")service.act("mkdir",{path:service.path,name:entry.text});
        else if(action==="rename")service.act("rename",{path:editor.target,name:entry.text});
        else if(action==="trash")service.act("trash",{paths:selectedPaths.slice()});
    }
    function displayCrumbs(){const rows=service.breadcrumbs,home=Quickshell.env("HOME"),index=rows.findIndex(r=>r.path===home);return (index>=0?rows.slice(index):rows).map(r=>({path:r.path,name:r.path===home?"Home":r.path==="/"?"Computer":r.name}));}
    function sizeText(item){if(item.directory)return "—";const n=item.size;return n<1024?n+" B":n<1048576?(n/1024).toFixed(1)+" KB":n<1073741824?(n/1048576).toFixed(1)+" MB":(n/1073741824).toFixed(1)+" GB";}
    function stepAction(delta){
        let current=-1;for(let i=0;i<actionRows.count;i++)if(actionRows.itemAt(i).activeFocus)current=i;
        for(let n=1;n<=actionRows.count;n++){const index=(current+n*delta+actionRows.count*2)%actionRows.count,item=actionRows.itemAt(index);if(item.enabled){item.forceActiveFocus();actions.contentItem.contentY=Math.max(0,Math.min(actions.contentItem.contentHeight-actions.contentItem.height,item.y-actions.contentItem.height/2));return;}}
    }
    function sortBy(key){if(service.sort===key)service.descending=!service.descending;else {service.sort=key;service.descending=key==="modified";}service.refresh();}
    onCurrentChanged:if(previewOpen || galleryMode)previewDelay.restart()
    onGalleryModeChanged:if(galleryMode)previewDelay.restart()
    onPreviewOpenChanged:{if(previewOpen)previewDelay.restart();else service.requestPreview("");}
    Timer {id:previewDelay;interval:140;onTriggered:root.service.requestPreview(root.current?root.current.path:"")}
    Connections {target:root.service
        function onInformationReady(details){root.information=details;infoWindow.show(details);}
        function onApplicationsReady(apps){root.applications=apps;openWith.open();}
        function onSelectedPath(path){
            if(path==="@preserve"){root.selectedPaths=root.selectedPaths.filter(p=>root.service.rows.some(r=>r.path===p));root.selected=root.service.rows.findIndex(r=>root.selectedPaths.includes(r.path));}
            else {root.selectedPaths=path?[path]:[];root.selected=root.service.rows.findIndex(r=>r.path===path);}
            root.saveTab();
        }
    }
    Connections {target:content.Window.window;function onActiveChanged(){if(content.Window.window.active)root.shell.activeFiles(root);}}
    Rectangle {id:windowMask;width:root.width;height:root.height;radius:Theme.nativeWindowFrame?0:12;layer.enabled:true;visible:false}
    Item {
        layer.enabled:true
        layer.effect:MultiEffect {maskEnabled:true;maskSource:windowMask}
        id:content;anchors.fill:parent;focus:true
        Keys.onPressed:event=>{
            const ctrl=event.modifiers & Qt.ControlModifier,shift=event.modifiers & Qt.ShiftModifier,alt=event.modifiers & Qt.AltModifier;
            let handled=true;
            if(ctrl && event.key===Qt.Key_W)root.closeTab();
            else if(ctrl && event.key===Qt.Key_T)root.newTab();
            else if(ctrl && !shift && event.key===Qt.Key_N)root.shell.newFilesWindow(root.service.path);
            else if(ctrl && event.key===Qt.Key_Tab)root.switchTab((root.activeTab+(shift?-1:1)+root.tabs.length)%root.tabs.length);
            else if(ctrl && event.key===Qt.Key_A)root.selectAll();
            else if(ctrl && event.key===Qt.Key_Z)root.service.undo();
            else if(ctrl && event.key===Qt.Key_I && root.current)root.service.send("info",{path:root.current.path});
            else if(ctrl && event.key===Qt.Key_L)root.dialogFor("goto");
            else if(ctrl && event.key===Qt.Key_F)filter.forceActiveFocus();
            else if(ctrl && shift && event.key===Qt.Key_N)root.dialogFor("mkdir");
            else if(ctrl && shift && (event.key===Qt.Key_Period || event.key===Qt.Key_Greater)){root.service.hidden=!root.service.hidden;root.service.refresh();}
            else if(ctrl && event.key===Qt.Key_C)root.stage("copy");
            else if(ctrl && event.key===Qt.Key_X)root.stage("move");
            else if(ctrl && event.key===Qt.Key_V)root.service.paste();
            else if(ctrl && event.key===Qt.Key_1)root.service.viewMode="grid";
            else if(ctrl && event.key===Qt.Key_2)root.service.viewMode="list";
            else if(ctrl && event.key===Qt.Key_3)root.service.viewMode="columns";
            else if(ctrl && event.key===Qt.Key_4)root.service.viewMode="gallery";
            else if(alt && event.key===Qt.Key_Left)root.service.back(-1);
            else if(alt && event.key===Qt.Key_Right)root.service.back(1);
            else if(alt && event.key===Qt.Key_Up)root.navigate(root.service.parentPath);
            else if(event.key===Qt.Key_F5)root.service.refresh();
            else if(event.key===Qt.Key_F2)root.dialogFor("rename");
            else if(event.key===Qt.Key_Delete)root.dialogFor("trash");
            else if(event.key===Qt.Key_Return || event.key===Qt.Key_Enter)root.openCurrent();
            else if(event.key===Qt.Key_Space && root.current)root.shell.quickLookWindow.show(root.current.path,root);
            else if(event.key===Qt.Key_Escape){if(root.previewOpen)root.previewOpen=false;else {root.selected=-1;root.selectedPaths=[];}}
            else if(event.key===Qt.Key_Down)root.step((root.listMode || root.galleryMode || root.columnsMode)?1:Math.max(1,Math.floor(grid.width/grid.cellWidth)),shift);
            else if(event.key===Qt.Key_Up)root.step((root.listMode || root.galleryMode || root.columnsMode)?-1:-Math.max(1,Math.floor(grid.width/grid.cellWidth)),shift);
            else if(event.key===Qt.Key_Right)root.step(1,shift);
            else if(event.key===Qt.Key_Left)root.step(-1,shift);
            else handled=false;
            event.accepted=handled;
        }
        Rectangle {x:root.sidebarWidth;width:parent.width-x;height:parent.height;color:Theme.content}
        Rectangle {width:root.sidebarWidth;height:parent.height;color:Theme.sidebarMaterial}
        Rectangle {x:root.sidebarWidth;width:parent.width-x;height:57;color:Theme.colors.background}
        MouseArea {x:0;y:0;width:parent.width;height:48;onPressed:if(content.Window.window)content.Window.window.startSystemMove()}
        WindowControls {
            windowName:"Files"
            windowActive:content.Window.window ? content.Window.window.active : false
            onTriggered: index => {
                const win=content.Window.window;
                if(index===0)root.visible=false;
                else if(win && index===1){
                    if(Quickshell.env("SWAYSOCK")){root.minimized=true;Quickshell.execDetached(["swaymsg",'[con_id='+root.swayContainer+'] move scratchpad']);}
                    else win.showMinimized();
                }
                else if(win){if(win.visibility===Window.FullScreen)win.showNormal();else win.showFullScreen();}
            }
        }
        Column {
            x:12;y:66;width:root.sidebarWidth-24;spacing:3
            PanelLabel {x:8;height:25;text:"Favourites";font.pixelSize:11;font.weight:Font.DemiBold;color:Theme.colors.muted}
            Repeater {
                model:[{name:"Home",folder:"",icon:"home"},{name:"Desktop",folder:"Desktop",icon:"screen"},{name:"Documents",folder:"Documents",icon:"document"},{name:"Downloads",folder:"Downloads",icon:"download"},{name:"Pictures",folder:"Pictures",icon:"folder"},{name:"Music",folder:"Music",icon:"music"},{name:"Videos",folder:"Videos",icon:"folder"},{name:"Data",folder:"data",icon:"folder"}]
                AbstractButton {
                    required property var modelData
                    width:parent.width;height:32;enabled:!root.service.busy
                    Accessible.name:modelData.name
                    onClicked:root.navigate(Quickshell.env("HOME")+(modelData.folder?"/"+modelData.folder:""))
                    background:Rectangle {radius:6;color:root.service.path===Quickshell.env("HOME")+(parent.modelData.folder?"/"+parent.modelData.folder:"")?Theme.selection:parent.hovered || parent.activeFocus?Theme.colors.hover:"transparent"}
                    contentItem:Item {
                        SystemGlyph {x:8;anchors.verticalCenter:parent.verticalCenter;width:18;height:18;name:modelData.icon;ink:Theme.colors.accent}
                        PanelLabel {x:34;width:parent.width-36;anchors.verticalCenter:parent.verticalCenter;text:modelData.name}
                    }
                }
            }
            Item {width:1;height:12}
            PanelLabel {x:8;height:23;text:"Locations";font.pixelSize:11;font.weight:Font.DemiBold;color:Theme.colors.muted}
            ControlRow {width:parent.width;text:"Computer";enabled:!root.service.busy;onClicked:root.navigate("/")}
            ControlRow {width:parent.width;text:"Trash";selected:root.service.path==="trash:";onClicked:root.navigate("trash:")}
            Repeater {model:root.service.volumes
                Row {required property var modelData;width:parent.width
                    ControlRow {width:parent.width-26;text:modelData.name;onClicked:root.navigate(modelData.path)}
                    ControlRow {width:26;text:"⏏";enabled:modelData.eject;Accessible.name:"Eject "+modelData.name;onClicked:root.service.act("eject",{path:modelData.path})}
                }
            }
        }
        Row {
            x:root.sidebarWidth+10;y:13;spacing:3
            Repeater {
                model:[{icon:"chevron-left",label:"Back",action:"back"},{icon:"chevron-right",label:"Forward",action:"forward"}]
                AbstractButton {
                    required property var modelData;width:27;height:30
                    enabled:!root.service.busy && (modelData.action==="back"?root.service.historyIndex>0:root.service.historyIndex<root.service.history.length-1)
                    Accessible.name:modelData.label
                    onClicked:root.service.back(modelData.action==="back"?-1:1)
                    background:Rectangle {radius:6;color:parent.hovered || parent.activeFocus?Theme.colors.hover:"transparent"}
                    contentItem:Item {SystemGlyph {anchors.centerIn:parent;width:17;height:17;name:modelData.icon;opacity:parent.parent.enabled?1:0.3}}
                }
            }
        }
        PanelLabel {x:root.sidebarWidth+73;y:14;width:Math.max(50,parent.width-x-195);height:29;verticalAlignment:Text.AlignVCenter;text:root.service.path==="trash:"?"Trash":root.service.path.split("/").filter(Boolean).pop() || "Computer";textFormat:Text.PlainText;font.pixelSize:15;font.weight:Font.DemiBold}
        Row {
            anchors.right:parent.right;anchors.rightMargin:14;y:13;spacing:4
            DesktopComboBox {width:112;height:30;model:["Icons","List","Columns","Gallery"];currentIndex:["grid","list","columns","gallery"].indexOf(root.service.viewMode);Accessible.name:"Files view";onActivated:index=>root.service.viewMode=["grid","list","columns","gallery"][index]}
            Repeater {
                model:[{icon:"info",label:"Preview",action:"preview"},{icon:"more",label:"File actions",action:"menu"}]
                AbstractButton {
                    required property var modelData;width:30;height:30
                    Accessible.name:modelData.label
                    onClicked:{if(modelData.action==="grid")root.service.viewMode="grid";else if(modelData.action==="list")root.service.viewMode="list";else if(modelData.action==="preview")root.previewOpen=!root.previewOpen;else {actions.x=root.width-230;actions.y=48;actions.open();}}
                    background:Rectangle {radius:7;color:parent.hovered || parent.activeFocus || (parent.modelData.action==="list" && root.listMode) || (parent.modelData.action==="grid" && !root.listMode) || (parent.modelData.action==="preview" && root.previewOpen)?Theme.selection:"transparent"}
                    contentItem:Item {SystemGlyph {anchors.centerIn:parent;width:17;height:17;name:modelData.icon}}
                }
            }
        }
        Row {
            x:root.sidebarWidth+10;y:58;height:root.tabHeight;visible:root.tabs.length>1;width:parent.width-x-10
            Repeater {model:root.tabs
                AbstractButton {required property var modelData;required property int index
                    width:(parent.width-30)/root.tabs.length;height:28
                    Accessible.name:modelData.path;Accessible.role:Accessible.PageTab;Accessible.selected:index===root.activeTab
                    onClicked:root.switchTab(index)
                    background:Rectangle {radius:6;color:parent.index===root.activeTab?Theme.colors.surface:parent.hovered?Theme.colors.hover:Theme.colors.field;border.color:parent.activeFocus?Theme.colors.accent:Theme.hairline}
                    contentItem:PanelLabel {text:modelData.path==="trash:"?"Trash":modelData.path.split("/").pop() || "Computer";horizontalAlignment:Text.AlignHCenter;verticalAlignment:Text.AlignVCenter;font.pixelSize:12;leftPadding:8;rightPadding:8}
                }
            }
            ControlRow {width:28;height:28;text:"+";Accessible.name:"New tab";onClicked:root.newTab()}
        }
        Item {id:dragGhost;width:1;height:1
            Drag.active:root.dragging
            Drag.dragType:Drag.Automatic
            Drag.supportedActions:Qt.CopyAction
            Drag.mimeData:({"text/uri-list":root.dragUris.join("\r\n")})
            Drag.onDragFinished:Qt.callLater(()=>root.dragging=false)
        }
        FilesDropArea {id:fileDrop;x:files.x;y:files.y;width:files.width;height:files.height;z:100;enabled:root.service.writable
            currentFolder:root.service.path
            onOpenFolder:path=>root.navigate(path)
            onFilesDropped:(urls,destination)=>root.service.drop(urls,destination)
        }
        Row {x:root.sidebarWidth+14;y:parent.height-98;visible:!!root.service.currentJob;width:parent.width-x-14;height:36;spacing:8
            PanelLabel {width:parent.width-78;anchors.verticalCenter:parent.verticalCenter;font.pixelSize:11;textFormat:Text.PlainText
                text:root.service.currentJob?root.service.currentJob.operation+" · "+root.service.currentJob.completed+"/"+root.service.currentJob.total+" · "+root.service.currentJob.state+(root.service.working?" · "+(root.service.currentJob.bytes/1048576).toFixed(1)+" MB":""):""
                Accessible.name:text
            }
            DialogButton {width:70;height:26;visible:root.service.working;text:"Cancel";onClicked:root.service.cancelJob()}
        }
        Popup {id:openWith;width:Math.min(360,root.width-40);height:Math.min(380,root.height-80);x:(root.width-width)/2;y:(root.height-height)/2;modal:true;focus:true;padding:12
            background:Rectangle {radius:12;color:Theme.colors.background;border.color:Theme.outline}
            contentItem:ListView {clip:true;model:root.applications;ScrollBar.vertical:DesktopScrollBar {}
                delegate:ControlRow {required property var modelData;width:parent.width;text:modelData.name;onClicked:{openWith.close();if(root.current)root.service.act("open-with",{path:root.current.path,application:modelData.id});}}
                PanelLabel {visible:root.applications.length===0;text:"No applications registered for this file";width:parent.width;wrapMode:Text.WordWrap}
            }
        }
        TextField {
            placeholderTextColor:Theme.colors.muted;selectionColor:Theme.colors.accent;selectedTextColor:Theme.colors.selectedText

            id:filter;x:root.sidebarWidth+14;y:66+root.tabHeight;width:parent.width-x-14;height:30
            placeholderText:"Filter this folder";font.family:Theme.font;font.pixelSize:12;color:Theme.colors.text;selectByMouse:true
            Accessible.name:"Filter this folder"
            background:Rectangle {radius:7;color:Theme.colors.background;border.color:filter.activeFocus?Theme.colors.accent:Theme.hairline}
            onTextEdited:filterDelay.restart()
            Keys.onDownPressed:{content.forceActiveFocus();root.step(1);}
            onAccepted:{root.service.query=text;root.service.refresh();content.forceActiveFocus();}
        }
        Timer {id:filterDelay;interval:220;onTriggered:{root.service.query=filter.text;root.service.refresh();}}
        PanelLabel {
            id:message;x:root.sidebarWidth+14;y:103+root.tabHeight;width:parent.width-x-14;height:visible?48:0
            visible:root.service.error!=="" || root.service.warning!==""
            text:root.service.error || root.service.warning;wrapMode:Text.WrapAnywhere;textFormat:Text.PlainText;font.pixelSize:12;color:Theme.colors.muted;elide:Text.ElideRight;maximumLineCount:3
            MouseArea {anchors.fill:parent;onClicked:errorDetails.open()}
            Accessible.name:"Operation details: "+text
            Accessible.onPressAction:errorDetails.open()
        }
        Popup {id:errorDetails;modal:true;focus:true;width:Math.min(600,content.width-40);height:Math.min(400,content.height-40);anchors.centerIn:parent;padding:18
            background:Rectangle {radius:12;color:Theme.colors.background;border.color:Theme.outline}
            contentItem:Column {spacing:12
                PanelLabel {text:"Operation Details";font.weight:Font.DemiBold}
                ScrollView {width:parent.width;height:errorDetails.height-115
                    TextArea {text:root.service.error || root.service.warning;readOnly:true;selectByMouse:true;wrapMode:TextEdit.Wrap;color:Theme.colors.text;font.family:Theme.font;background:Rectangle {color:"transparent"}}
                }
                DialogButton {width:100;text:"Done";onClicked:errorDetails.close()}
            }
        }
        Item {
            id:files;x:root.sidebarWidth+10;y:message.visible?message.y+message.height+8:105+root.tabHeight
            width:parent.width-x-10-(root.previewOpen?Math.min(280,(parent.width-root.sidebarWidth)*0.43):0)
            height:parent.height-y-64-(root.service.currentJob?40:0)
            enabled:!root.service.busy
            Row {
                id:headings;visible:root.listMode;width:parent.width;height:26
                Repeater {
                    model:[{key:"name",label:"Name",fraction:0.55},{key:"modified",label:"Modified",fraction:0.27},{key:"size",label:"Size",fraction:0.18}]
                    ControlRow {required property var modelData;width:headings.width*modelData.fraction;height:26;topPadding:3;bottomPadding:3;text:modelData.label+(root.service.sort===modelData.key?(root.service.descending?" ↓":" ↑"):"");onClicked:root.sortBy(modelData.key)}
                }
            }
            ListView {
                id:list;visible:root.listMode;anchors.top:headings.bottom;anchors.bottom:parent.bottom;width:parent.width;clip:true;model:root.service.rows
                ScrollBar.vertical:DesktopScrollBar {}
                delegate:Rectangle {
                    required property var modelData;required property int index
                    width:list.width;height:31;radius:5;color:root.isSelected(index)?root.selectionFill:index%2?Theme.colors.background:"transparent"
                    SystemGlyph {x:8;anchors.verticalCenter:parent.verticalCenter;name:modelData.directory?"folder":"document";width:18;height:18;ink:root.isSelected(index)?root.selectionText:Theme.colors.accent}
                    PanelLabel {x:34;anchors.verticalCenter:parent.verticalCenter;width:parent.width*.55-40;text:(modelData.link?"↗ ":"")+modelData.name;textFormat:Text.PlainText;color:root.isSelected(index)?root.selectionText:Theme.colors.text;font.pixelSize:12}
                    PanelLabel {x:parent.width*.55+8;anchors.verticalCenter:parent.verticalCenter;width:parent.width*.27-10;text:Qt.formatDateTime(new Date(modelData.mtime*1000),"dd MMM yyyy");font.pixelSize:11;color:root.isSelected(index)?root.selectionText:Theme.colors.muted}
                    PanelLabel {x:parent.width*.82+8;anchors.verticalCenter:parent.verticalCenter;width:parent.width*.18-12;text:root.sizeText(modelData);font.pixelSize:11;color:root.isSelected(index)?root.selectionText:Theme.colors.muted}
                    FolderDropTarget {anchors.fill:parent;enabled:modelData.directory && root.service.path!=="trash:";folder:modelData.path;coordinator:fileDrop}
                    MouseArea {anchors.fill:parent;acceptedButtons:Qt.LeftButton|Qt.RightButton;drag.target:dragGhost
                        onPressed:event=>{if(event.button===Qt.LeftButton && !event.modifiers && !root.isSelected(index))root.choose(index,event.modifiers);}
                        onPositionChanged:if(drag.active && !root.dragging){root.dragUris=root.selectedUris();Qt.callLater(root.beginDrag);}
                        onClicked:event=>{if(event.button!==Qt.RightButton || !root.isSelected(index))root.choose(index,event.modifiers);if(event.button===Qt.RightButton){actions.x=Math.min(root.width-actions.width-8,mapToItem(content,event.x,event.y).x);actions.y=Math.min(root.height-actions.height-8,mapToItem(content,event.x,event.y).y);actions.open();}} onDoubleClicked:root.openCurrent()}
                    Accessible.name:modelData.name;Accessible.role:Accessible.ListItem;Accessible.selected:root.isSelected(index);Accessible.onPressAction:root.choose(index,0)
                }
            }
            GridView {
                id:grid;visible:root.service.viewMode==="grid";anchors.fill:parent;cellWidth:108;cellHeight:108;clip:true;model:root.service.rows
                ScrollBar.vertical:DesktopScrollBar {}
                delegate:Item {
                    required property var modelData;required property int index
                    width:108;height:108
                    Accessible.name:modelData.name;Accessible.role:Accessible.ListItem;Accessible.selected:root.isSelected(index);Accessible.onPressAction:root.choose(index,0)
                    Rectangle {x:20;y:4;width:68;height:62;radius:8;color:root.isSelected(index)?Theme.selection:"transparent"}
                    Image {visible:modelData.directory;x:25;y:9;width:58;height:48;source:"icons/folder.svg";sourceSize:Qt.size(116,96)}
                    Image {id:thumbnail;property bool failed:false;onStatusChanged:if(status===Image.Error)failed=true;visible:!modelData.directory && ["image/png","image/jpeg","image/webp","image/gif","image/bmp"].includes(modelData.kind) && modelData.size<=33554432 && !failed;x:24;y:8;width:60;height:54;source:visible?modelData.uri:"";sourceSize:Qt.size(120,108);fillMode:Image.PreserveAspectFit;asynchronous:true}
                    SystemGlyph {visible:!modelData.directory && !thumbnail.visible;x:36;y:14;width:36;height:40;name:"document";ink:Theme.colors.muted}
                    PanelLabel {visible:modelData.link;x:22;y:45;text:"↗";font.pixelSize:16}
                    Rectangle {x:3;y:70;width:102;height:30;radius:4;color:root.isSelected(index)?root.selectionFill:"transparent"}
                    PanelLabel {x:5;y:70;width:98;height:30;text:modelData.name;textFormat:Text.PlainText;horizontalAlignment:Text.AlignHCenter;verticalAlignment:Text.AlignVCenter;wrapMode:Text.Wrap;maximumLineCount:2;font.pixelSize:12;color:root.isSelected(index)?root.selectionText:Theme.colors.text}
                    FolderDropTarget {anchors.fill:parent;enabled:modelData.directory && root.service.path!=="trash:";folder:modelData.path;coordinator:fileDrop}
                    MouseArea {anchors.fill:parent;acceptedButtons:Qt.LeftButton|Qt.RightButton;drag.target:dragGhost
                        onPressed:event=>{if(event.button===Qt.LeftButton && !event.modifiers && !root.isSelected(index))root.choose(index,event.modifiers);}
                        onPositionChanged:if(drag.active && !root.dragging){root.dragUris=root.selectedUris();Qt.callLater(root.beginDrag);}
                        onClicked:event=>{if(event.button!==Qt.RightButton || !root.isSelected(index))root.choose(index,event.modifiers);if(event.button===Qt.RightButton){actions.x=Math.min(root.width-actions.width-8,mapToItem(content,event.x,event.y).x);actions.y=Math.min(root.height-actions.height-8,mapToItem(content,event.x,event.y).y);actions.open();}} onDoubleClicked:root.openCurrent()}
                }
            }
            ColumnView {selectionActive:root.selectionActive;dropCoordinator:fileDrop;visible:root.columnsMode;anchors.fill:parent;columns:root.service.columns;currentPath:root.current?root.current.path:"";onChosen:(path,directory)=>{if(directory)root.navigate(path);else {const index=root.service.rows.findIndex(r=>r.path===path);if(index>=0)root.choose(index,0);else {root.service.nextSelection=path;root.navigate(path.slice(0,path.lastIndexOf("/")) || "/");}}}}
            Item {visible:root.galleryMode;anchors.fill:parent
                FilePreview {x:12;y:0;width:parent.width-24;height:Math.max(60,parent.height-118);result:root.current?root.service.preview:{text:"Select an item"};active:root.visible && root.galleryMode;onPageRequested:number=>root.service.requestPreview(root.current.path,number)}
                ListView {id:galleryStrip;anchors.bottom:parent.bottom;width:parent.width;height:104;orientation:ListView.Horizontal;clip:true;spacing:5;model:root.service.rows;ScrollBar.horizontal:DesktopScrollBar {}
                    delegate:AbstractButton {required property var modelData;required property int index;width:92;height:92;
                        FolderDropTarget {anchors.fill:parent;enabled:modelData.directory && root.service.path!=="trash:";folder:modelData.path;coordinator:fileDrop}
Accessible.name:modelData.name;Accessible.selected:root.isSelected(index);onClicked:root.choose(index,0);onDoubleClicked:root.openCurrent()
                        background:Rectangle {radius:7;color:root.isSelected(index)?Theme.selection:parent.hovered?Theme.colors.hover:"transparent";border.color:parent.activeFocus?Theme.colors.accent:"transparent"}
                        contentItem:Item {
                            Image {id:galleryThumb;anchors.horizontalCenter:parent.horizontalCenter;y:4;width:72;height:46;source:modelData.directory?"icons/folder.svg":(["image/png","image/jpeg","image/webp","image/gif","image/bmp"].includes(modelData.kind) && modelData.size<=33554432?modelData.uri:"");sourceSize:Qt.size(144,92);fillMode:Image.PreserveAspectFit;asynchronous:true}
                            SystemGlyph {visible:!galleryThumb.source || galleryThumb.status===Image.Error;anchors.horizontalCenter:parent.horizontalCenter;y:5;width:36;height:42;name:"document";ink:Theme.colors.muted}
                            PanelLabel {y:52;width:parent.width;height:36;text:modelData.name;textFormat:Text.PlainText;wrapMode:Text.Wrap;maximumLineCount:2;horizontalAlignment:Text.AlignHCenter;font.pixelSize:11}}
                    }
                }
            }
            PanelLabel {anchors.centerIn:parent;visible:root.service.rows.length===0;text:root.service.busy?"Loading…":root.service.query?"No matching items":"This folder is empty";color:Theme.colors.muted}
        }
        Item {
            id:previewPane;visible:root.previewOpen;x:files.x+files.width+8;y:files.y;width:parent.width-x-14;height:files.height
            Rectangle {width:1;height:parent.height;color:Theme.hairline}
            Column {
                x:12;width:parent.width-12;spacing:10
                PanelLabel {width:parent.width;text:root.current?root.current.name:"Select an item";textFormat:Text.PlainText;wrapMode:Text.WrapAnywhere;maximumLineCount:3;font.weight:Font.DemiBold}
                PanelLabel {width:parent.width;text:root.current?(root.current.directory?"Folder":root.sizeText(root.current)):"";color:Theme.colors.muted;font.pixelSize:11}
                Image {visible:root.service.preview.type==="image";width:parent.width;height:visible?Math.max(80,previewPane.height-105):0;source:visible?root.service.preview.uri || "":"";sourceSize:Qt.size(800,800);fillMode:Image.PreserveAspectFit;asynchronous:true;PanelLabel {visible:parent.status===Image.Error;anchors.centerIn:parent;text:"Preview unavailable";color:Theme.colors.muted}}
                Flickable {
                    visible:root.service.preview.type!=="image";width:parent.width;height:Math.max(40,previewPane.height-110);clip:true;contentHeight:previewText.height;contentWidth:width
                    PanelLabel {id:previewText;width:parent.width;text:root.current?root.service.preview.text || "Loading preview…":"";textFormat:Text.PlainText;wrapMode:Text.WrapAnywhere;elide:Text.ElideNone;font.pixelSize:12}
                }
                PanelLabel {visible:!!root.service.preview.truncated;text:"Preview shortened";font.pixelSize:10;color:Theme.colors.muted}
            }
        }
        Flickable {
            id:pathbar;onContentWidthChanged:Qt.callLater(()=>contentX=Math.max(0,contentWidth-width));x:root.sidebarWidth+12;y:parent.height-57;width:parent.width-x-12;height:25;contentWidth:crumbs.width;clip:true
            Row {
                id:crumbs;spacing:1
                Repeater {
                    model:root.displayCrumbs()
                    ControlRow {required property var modelData;text:modelData.name+"  ›";height:25;topPadding:3;bottomPadding:3;width:Math.min(220,Math.max(40,labelMetrics.advanceWidth(modelData.name)+46));enabled:!root.service.busy;onClicked:root.navigate(modelData.path)}
                }
            }
        }
        FontMetrics {id:labelMetrics;font.family:Theme.font;font.pixelSize:13}
        PanelLabel {x:root.sidebarWidth+14;y:parent.height-28;width:parent.width-x-14;height:20;text:root.service.busy?"Loading…":root.selectedPaths.length?root.selectedPaths.length+" selected · "+root.service.rows.length+" items":root.service.stagedPath?(root.service.stagedAction==="move"?"Move":"Copy")+" ready · Navigate to a destination and Paste":root.service.rows.length+(root.service.rows.length===1?" item":" items")+(root.service.hidden?" · Hidden items shown":"");font.pixelSize:11;color:Theme.colors.muted}
        Popup {
            id:actions;focus:true;width:220;height:Math.min(550,root.height-70);padding:6;clip:true
            background:Rectangle {radius:10;color:Theme.colors.background;border.color:Theme.outline}
            contentItem:Flickable {contentHeight:actionColumn.height;clip:true;focus:true
              Keys.onDownPressed:root.stepAction(1)
              Keys.onUpPressed:root.stepAction(-1)
              ScrollBar.vertical:DesktopScrollBar {}
              Column {id:actionColumn;width:208
                Repeater {id:actionRows
                    model:[{label:"Open",action:"open",enabled:!!root.current},{label:"Open With…",action:"open-with",enabled:!!root.current && !root.current.directory},{separator:true,label:""},{label:"New Tab",action:"new-tab",enabled:true},{label:"New Window",action:"new-window",enabled:true},{label:"New Folder…",action:"mkdir",enabled:root.service.writable},{label:"Rename…",action:"rename",enabled:root.selectedPaths.length===1 && root.service.path!=="trash:"},{separator:true,label:""},{label:"Copy",action:"copy",enabled:!!root.current},{label:"Cut",action:"move",enabled:!!root.current},{label:"Paste",action:"paste",enabled:root.service.writable},{label:"Paste Keeping Both",action:"paste-both",enabled:root.service.writable},{label:"Undo Last Move",action:"undo",enabled:root.service.canUndo},{separator:true,label:""},{label:"Put Back",action:"restore",enabled:root.service.path==="trash:" && !!root.current},{label:"Move to Trash…",action:"trash",enabled:root.service.path!=="trash:" && !!root.current},{separator:true,label:""},{label:"Go to Folder…",action:"goto",enabled:true},{label:root.service.hidden?"Hide Hidden Files":"Show Hidden Files",action:"hidden",enabled:true},{label:"Refresh",action:"refresh",enabled:true}]
                    ControlRow {
                        required property var modelData;width:208;height:modelData.separator?9:30;text:modelData.label;Rectangle {visible:!!modelData.separator;x:8;width:parent.width-16;height:1;anchors.verticalCenter:parent.verticalCenter;color:Theme.hairline}enabled:!modelData.separator && modelData.enabled && !root.service.busy
                        onClicked:{
                            actions.close();const a=modelData.action;
                            if(a==="open")root.openCurrent();else if(a==="copy" || a==="move")root.stage(a);else if(a==="paste")root.service.paste();
                            else if(a==="paste-both")root.service.paste("keep-both");
                            else if(a==="undo")root.service.undo();
                            else if(a==="new-tab")root.newTab();
                            else if(a==="new-window")root.shell.newFilesWindow(root.service.path);
                            else if(a==="restore")root.service.startJob("restore",root.selectedPaths);
                            else if(a==="open-with")root.service.act("applications",{path:root.current.path});
                            else if(a==="hidden"){root.service.hidden=!root.service.hidden;root.service.refresh();}
                            else if(a==="refresh")root.service.refresh();else root.dialogFor(a);
                        }
                    }
                }
            }
        }
        }
        Popup {
            id:editor;property string action:"";property string target:"";property string label:""
            modal:true;focus:true;width:Math.min(430,root.width-40);x:(root.width-width)/2;y:Math.max(30,(root.height-height)/2);padding:20
            background:Rectangle {radius:14;color:Theme.colors.background;border.color:Theme.outline}
            contentItem:Column {
                spacing:14
                PanelLabel {width:parent.width;text:editor.action==="trash"?"Move to Trash?":editor.action==="rename"?"Rename":editor.action==="goto"?"Go to Folder":"New Folder";font.pixelSize:18;font.weight:Font.DemiBold}
                PanelLabel {visible:editor.action==="trash";width:parent.width;text:editor.label+" will be moved to Trash.";textFormat:Text.PlainText;wrapMode:Text.WrapAnywhere}
                TextField {
            placeholderTextColor:Theme.colors.muted;selectionColor:Theme.colors.accent;selectedTextColor:Theme.colors.selectedText
id:entry;visible:editor.action!=="trash";width:parent.width;height:34;color:Theme.colors.text;font.family:Theme.font;selectByMouse:true;placeholderText:editor.action==="goto"?"Folder path":"Name";Accessible.name:placeholderText;background:Rectangle{radius:6;color:Theme.colors.field;border.color:Theme.colors.accent} onAccepted:if(text)root.submit()}
                Row {
                    spacing:8
                    DialogButton {id:cancelEditor;width:110;text:"Cancel";defaultAction:editor.action==="trash";onClicked:editor.close()}
                    DialogButton {width:160;destructive:editor.action==="trash";primary:editor.action!=="trash";text:editor.action==="trash"?"Move to Trash":editor.action==="goto"?"Go":editor.action==="mkdir"?"Create":"Rename";enabled:editor.action==="trash" || entry.text.length>0;onClicked:root.submit()}
                }
            }
        }
    }
}

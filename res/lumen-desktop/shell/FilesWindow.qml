import QtQuick
import QtQuick.Controls
import QtQuick.Window
import QtQuick.Effects
import Quickshell
import "FilesTabModel.js" as TabModel
import "FilesHistoryModel.js" as HistoryModel
import "FileLayoutModel.js" as LayoutModel
import "FileLinks.js" as FileLinks
import "FileShareIntent.js" as FileShareIntent

FloatingWindow {
    id:root
    readonly property bool selectionActive:content.Window.window?content.Window.window.active:false
    readonly property color selectionFill:selectionActive?Theme.colors.accent:Theme.selection
    readonly property color selectionText:selectionActive?Theme.colors.selectedText:Theme.colors.text
    required property var shell
    required property var service
    NetworkLocationsState {id:networkLocations;active:networkPopup.visible}
    TrashActionsState {id:trashActions;active:root.visible && root.service.path==="trash:";onChanged:root.service.refresh()}
    property int windowNumber:1
    title:"Files"+(windowNumber>1?" "+windowNumber:"")+" — "+(service.path==="trash:"?"Trash":service.path.split("/").pop() || "Computer")
    visible:false
    implicitWidth:900;implicitHeight:600
    minimumSize:Qt.size(480,320)
    readonly property int labelRowHeight:31+Theme.textRole("label")-12
    readonly property int footerHeight:20+Theme.textRole("metadata")-11
    readonly property bool compactMetadata:width-sidebarWidth<480*Theme.textScale
    color:"transparent"
    surfaceFormat.opaque:false
    property bool minimized:false
    property var swayContainer:0
    readonly property bool listMode:service.viewMode==="list"
    readonly property bool galleryMode:service.viewMode==="gallery"
    readonly property bool columnsMode:service.viewMode==="columns"
    property int selected:-1
    property var selectedPaths:[]
    property var bandPreviewPaths:null
    readonly property real selectionGutter:12
    readonly property bool bandPermitted:visible && selectionActive && !service.busy && (!shell.screenSaverPreferences || shell.screenSaverPreferences.permitted)
    readonly property string bandContext:JSON.stringify([service.path,activeTabId,service.viewMode,service.query,service.sort,service.descending,service.hidden,service.groupBy,service.foldersFirst,screen?screen.name:""])
    function cancelPendingPresentation(){if(restoringPresentationState){++presentationRevision;restoringPresentationState=false;pendingPresentation=null;}}
    function commitBand(value){if(!value)return;cancelPendingPresentation();bandPreviewPaths=null;selectedPaths=value.paths;selected=value.current;selectionAnchor=value.anchor;content.forceActiveFocus();saveTab();}
    function cancelBand(reason){bandPreviewPaths=null;if(reason==="escape" && selectionActive)content.forceActiveFocus();}
    property int selectionAnchor:-1
    property int tabSerial:1
    property var tabs:[TabModel.create("tab-1",Quickshell.env("HOME"))]
    readonly property string activeTabId:(tabs[activeTab] || {}).id || ""
    property string restoringTabId:""
    property int activeTab:0
    readonly property int tabHeight:tabs.length>1?Math.ceil(documentTabs.implicitHeight):0
    property string tagsTarget:""
    property var tagsIdentity:({})
    property bool tagsSupported:false
    property string tagsError:""
    property string applicationsPath:""
    property var applicationsSelection:null
    property var selectedTags:[]
    readonly property var colourTags:[{id:"red",label:"Red",color:"#e54b4b"},{id:"orange",label:"Orange",color:"#e79735"},{id:"yellow",label:"Yellow",color:"#d6b82f"},{id:"green",label:"Green",color:"#50a86b"},{id:"blue",label:"Blue",color:"#438ed6"},{id:"purple",label:"Purple",color:"#9b6ac6"},{id:"grey",label:"Grey",color:"#929298"}]
    function editTags(){if(!current)return;tagsTarget=current.path;tagsIdentity=current.identity;tagsSupported=current.tagsSupported;tagsError=current.tagsError;selectedTags=(current.tags || []).map(tag=>tag.id);tagsEditor.open();}
    property var applications:[]
    property bool dragging:false
    property var information:({})
    FileInfoWindow {id:infoWindow;owner:root}
    property alias informationWindow:infoWindow
    function observeOwnedFocus(){if(!shell.ownedContext)return;if(selectionActive){shell.ownedContext.focus(root,root,"lumen-files","main");shell.activeFiles(root);}else shell.ownedContext.leave(root);}
    onSelectionActiveChanged:observeOwnedFocus()
    Component.onDestruction:if(shell.ownedContext)shell.ownedContext.close(root)
    property var dragUris:[]
    function beginDrag(){if(dragUris.length)dragging=true;}
    property bool presentationTransition:false
    property bool restoringPresentationState:false
    property var pendingPresentation:null
    property int presentationRevision:0
    function viewSnapshot(view,horizontal){
        const value=horizontal?view.contentX:view.contentY;
        const index=view.indexAt(horizontal?value+1:1,horizontal?1:value+1);
        const item=index>=0?view.itemAtIndex(index):null;
        return {offset:value,anchor:HistoryModel.ref(service.rows[index]),delta:item?Math.max(0,value-(horizontal?item.x:item.y)):0};
    }
    function historyPresentation(){return {view:service.viewMode,query:service.query,selection:selectedPaths.map(path=>service.rows.find(row=>row.path===path)),current:current,anchor:service.rows[selectionAnchor],scroll:{list:viewSnapshot(list,false),grid:viewSnapshot(grid,false),gallery:viewSnapshot(galleryStrip,true),columns:columnView.snapshot()}};}
    function captureHistory(){
        if(presentationTransition || restoringPresentationState || service.busy || restoringTabId===activeTabId || service.rowsPath!==service.path)return;
        service.history=HistoryModel.save(service.history,service.historyIndex,historyPresentation());
    }
    function restoreScroll(view,value,horizontal){
        value=value || {};const index=HistoryModel.locate(service.rows,value.anchor);
        if(index>=0)view.positionViewAtIndex(index,ListView.Beginning);
        const offset=index>=0?(horizontal?view.contentX:view.contentY)+(value.delta || 0):value.offset || 0;
        if(horizontal)view.contentX=Math.max(0,Math.min(offset,view.contentWidth-view.width));
        else view.contentY=Math.max(0,Math.min(offset,view.contentHeight-view.height));
    }
    function restorePresentation(saved){
        pendingPresentation=saved;restoringPresentationState=true;const revision=++presentationRevision;
        if(saved){if(saved.selectionOverflow)service.warning="The previous selection exceeded the navigation history limit and was cleared. Select the items again.";service.restoreView(saved.view);filter.text=service.query;const selectedState=HistoryModel.reconcile(saved,service.rows);if(!saved.selectionOverflow && ((saved.selection || []).length>selectedState.paths.length || (saved.current && selectedState.selected<0)))service.warning="Some selected or focused items changed or could not be followed. Select them again.";selectedPaths=selectedState.paths;selected=selectedState.selected;selectionAnchor=selectedState.anchor;}
        else {selectedPaths=[];selected=-1;selectionAnchor=-1;}
        Qt.callLater(()=>{if(revision!==presentationRevision)return;if(service.busy){restoringPresentationState=false;return;}const scroll=(saved || {}).scroll || {};restoreScroll(list,scroll.list,false);restoreScroll(grid,scroll.grid,false);restoreScroll(galleryStrip,scroll.gallery,true);columnView.restoreSnapshot(scroll.columns || {});restoringPresentationState=false;saveTab();});
    }
    function newTab(){saveTab();const current=tabs[activeTab];const tab=TabModel.create("tab-"+(++tabSerial),current?current.path:service.path);tab.viewMode=service.viewMode;tabs=tabs.concat([tab]);switchTab(tabs.length-1);}
    function saveTab(){
        if(!activeTabId || (restoringTabId===activeTabId && service.busy))return;
        captureHistory();
        tabs=TabModel.save(tabs,activeTabId,{path:service.path,history:service.history,historyIndex:service.historyIndex,selectedPaths:selectedPaths,selectedPath:current?current.path:"",selectionAnchor:selectionAnchor,query:filter.text,viewMode:service.viewMode});
    }
    function restoreTab(){
        const tab=tabs[activeTab];if(!tab)return;
        restoringTabId=tab.id;filterDelay.stop();selected=-1;selectedPaths=[];selectionAnchor=-1;
        service.prepareTab(tab.path);service.history=HistoryModel.clone(tab.history);service.historyIndex=tab.historyIndex;service.restoreView(tab.viewMode || "grid");service.query=tab.query || "";filter.text=service.query;
        service.navigate(tab.path,tab.historyIndex);
    }
    function switchTab(index){if(index===activeTab || index<0 || index>=tabs.length)return;saveTab();activeTab=index;restoreTab();}
    function reorderTab(id,target){const active=activeTabId;saveTab();tabs=TabModel.reorder(tabs,id,target);activeTab=TabModel.indexOf(tabs,active);}
    function closeTabId(id){
        if(TabModel.indexOf(tabs,id)<0)return;
        if(tabs.length<2){visible=false;return;}
        const active=activeTabId;saveTab();const next=TabModel.remove(tabs,id,active);tabs=next.tabs;activeTab=TabModel.indexOf(tabs,next.activeId);
        if(active!==next.activeId)restoreTab();
    }
    function closeTab(){closeTabId(activeTabId);}
    function duplicateSelected(){if(service.writable && service.path!=="trash:" && selectedPaths.length && !service.working)service.startJob("copy",selectedPaths,service.path,"keep-both");}
    function selectedUris(){return selectedPaths.map(p=>"file://"+p.split("/").map(encodeURIComponent).join("/"));}
    function isSelected(index){const paths=bandPreviewPaths===null?selectedPaths:bandPreviewPaths;return service.rows[index] && paths.includes(service.rows[index].path);}
    function selectAll(){cancelPendingPresentation();selectedPaths=service.rows.map(r=>r.path);selected=service.rows.length?0:-1;}
    onVisibleChanged:{service.active=visible;if(!visible){previewDelay.stop();service.requestPreview("");}else if(previewOpen || galleryMode)schedulePreview();if(!visible && shell.ownedContext)shell.ownedContext.close(root)}

    readonly property var defaultFavorites:[{name:"Home",path:Quickshell.env("HOME")},{name:"Desktop",path:Quickshell.env("HOME")+"/Desktop"},{name:"Documents",path:Quickshell.env("HOME")+"/Documents"},{name:"Downloads",path:Quickshell.env("HOME")+"/Downloads"},{name:"Pictures",path:Quickshell.env("HOME")+"/Pictures"},{name:"Music",path:Quickshell.env("HOME")+"/Music"},{name:"Videos",path:Quickshell.env("HOME")+"/Videos"},{name:"Data",path:Quickshell.env("HOME")+"/data"}]
    readonly property var sidebarFavorites:service.favorites===null?defaultFavorites:service.favorites
    property int sidebarEditIndex:-1
    function editSidebar(action){let items=sidebarFavorites.slice();const index=sidebarEditIndex;if(index<0 || index>=items.length)return;if(action==="remove")items.splice(index,1);else {const target=index+(action==="up"?-1:1);if(target<0 || target>=items.length)return;const swap=items[index];items[index]=items[target];items[target]=swap;}service.favorites=items;sidebarActions.close();}
    function addSidebar(){if(service.path==="trash:" || sidebarFavorites.some(item=>item.path===service.path) || sidebarFavorites.length>=32)return;service.favorites=sidebarFavorites.concat([{name:service.path.split("/").filter(Boolean).pop() || "Computer",path:service.path}]);}
    property bool previewOpen:false
    property int previewDragWidth:-1
    readonly property var previewLimits:LayoutModel.previewLimits(width-sidebarWidth-24,Theme.textScale)
    readonly property bool previewVisible:previewOpen && previewLimits.enabled
    readonly property string previewMessage:previewOpen && !previewLimits.enabled?"Preview hidden: widen this window to keep both panes usable.":""
    readonly property int previewWidth:LayoutModel.previewWidth(previewDragWidth>=0?previewDragWidth:service.previewWidth,width-sidebarWidth-24,Theme.textScale)
    readonly property var current:service.rows[selected] || null
    property int sidebarDragWidth:-1
    readonly property int sidebarMaximum:LayoutModel.sidebarMaximum(width)
    readonly property int sidebarWidth:LayoutModel.sidebarWidth(sidebarDragWidth>=0?sidebarDragWidth:service.sidebarWidth,width)
    function groupCollapsed(name){return Array.isArray(service.collapsedSidebarGroups) && service.collapsedSidebarGroups.includes(name);}
    function collapseSidebar(name,value){let groups=Array.isArray(service.collapsedSidebarGroups)?service.collapsedSidebarGroups.slice():[];groups=groups.filter(group=>group!==name);if(value)groups.push(name);service.collapsedSidebarGroups=groups;}
    WindowGeometry {id:windowGeometry;ownerWindow:root;containerId:root.swayContainer}
    function fitScreen() {
        if(!visible || !screen || !Quickshell.env("SWAYSOCK"))return;
        windowGeometry.fitWorkspace(32,150);
    }

    Timer {id:fitTimer;interval:100;onTriggered:root.fitScreen()}
    Connections {target:root.shell.exportedMenu;function onStateChanged(){const state=root.shell.exportedMenu.state;if(state.appName==="org.quickshell" && state.title===root.title)root.swayContainer=state.windowId;}}
    onScreenChanged:Qt.callLater(root.fitScreen)
    Connections {target:root.screen;function onWidthChanged(){fitTimer.restart();}function onHeightChanged(){fitTimer.restart();}}
    function openPath(path,expectedFacts) {
        if(shell.claimPopup)shell.claimPopup("");
        if(minimized && Quickshell.env("SWAYSOCK")){
            Quickshell.execDetached(["swaymsg",'[con_id='+swayContainer+'] move workspace current, focus']);minimized=false;
        }
        const wasVisible=visible;visible=true;
        navigate(path || service.path,expectedFacts);
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
        else if(action==="duplicate")duplicateSelected();
        else if(action==="undo")service.undo();
        else if(action==="new-window")shell.newFilesWindow(service.path);
        else if(action==="new-tab")newTab();
        else if(["grid","list","columns","gallery"].includes(action))service.viewMode=action;
        else if(action==="info" && current)service.send("info",{path:current.path});
        else if(action==="make-link" && current)service.act("make-link",{path:current.path});
        else if(action==="tags")editTags();
        else if(action==="view-options")viewOptions.open();
        else if(action==="preview")previewOpen=!previewOpen;
        else if(action==="hidden"){service.hidden=!service.hidden;service.refresh();}
        else if(action==="refresh")service.refresh();
        else if(action==="back" || action==="forward")service.back(action==="back"?-1:1);
        else if(action==="up")navigate(service.parentPath);
        else if(action==="home")navigate(Quickshell.env("HOME"));
        else dialogFor(action);
    }
    function navigate(path,expectedFacts){captureHistory();restoringTabId="";filterDelay.stop();presentationTransition=true;filter.text="";service.query="";service.navigate(path,undefined,false,expectedFacts);presentationTransition=false;}
    function focusContents(){if(shell.claimPopup)shell.claimPopup("");if(content.Window.window)content.Window.window.requestActivate();content.forceActiveFocus();activationTimer.epoch=shell.focusEpoch;activationTimer.restart();}
    Timer {id:activationTimer;property int epoch;interval:100;onTriggered:{if(epoch!==root.shell.focusEpoch)return;const window=root.shell.windows.find(w=>w.appId==="org.quickshell" && w.title===root.title);if(window)root.shell.focusWindow(window);}}
    function quickLookStep(delta){step(delta,false);if(current)shell.quickLookWindow.show(current.path,root);}
    function openCurrent(){if(!current || service.busy)return;if(current.directory)navigate(current.path);else service.act("open",{path:current.path});}
    function choose(index,modifiers){
        const row=service.rows[index];if(!row)return;cancelPendingPresentation();
        if((modifiers || 0)&Qt.ShiftModifier){const from=selectionAnchor<0?index:selectionAnchor;selectedPaths=service.rows.slice(Math.min(from,index),Math.max(from,index)+1).map(r=>r.path);}
        else if((modifiers || 0)&Qt.ControlModifier){selectedPaths=selectedPaths.includes(row.path)?selectedPaths.filter(p=>p!==row.path):selectedPaths.concat([row.path]);selectionAnchor=index;}
        else {selectedPaths=[row.path];selectionAnchor=index;}
        selected=index;content.forceActiveFocus();
    }
    function step(delta,extend){choose(Math.max(0,Math.min(service.rows.length-1,selected+delta)),extend?Qt.ShiftModifier:0);if(listMode)list.positionViewAtIndex(selected,ListView.Contain);else if(galleryMode)galleryStrip.positionViewAtIndex(selected,ListView.Contain);else if(columnsMode)columnView.ensureVisible(current?current.path:"");else grid.positionViewAtIndex(selected,GridView.Contain);}
    function stage(action){if(selectedPaths.length)service.stage(selectedPaths,action);}
    function prepareDeletion(mode){
        if(service.path!=="trash:" || service.working || service.busy)return;
        const items=mode==="selected"?selectedPaths.map(path=>{const row=service.rows.find(item=>item.path===path);return {path:path,identity:row?row.identity:null};}):[];
        trashActions.prepare(mode,items);
    }
    function dialogFor(action){
        if(service.busy || (action==="rename" && service.working))return;
        if(["rename","trash"].includes(action) && !current)return;
        if(action==="rename" && selectedPaths.length>1){const captured=selectedPaths.map(path=>service.rows.find(row=>row.path===path)).filter(Boolean);if(captured.length!==selectedPaths.length){service.error="The selection changed. Select the items again.";return;}batchRename.begin(captured);return;}
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
    function schedulePreview(){service.requestPreview("");previewDelay.stop();if(visible && current && (previewOpen || galleryMode))previewDelay.restart();}
    onCurrentChanged:{
        const item=root.current;
        if(root.service.applicationRequestId>0 && (!item || item.path!==root.service.applicationRequestPath || JSON.stringify(item.actionFacts)!==JSON.stringify(root.service.applicationRequestFacts)))root.service.cancelApplications();
        if(root.applicationsSelection && (!item || item.path!==root.applicationsPath || JSON.stringify(item.actionFacts)!==JSON.stringify(root.applicationsSelection.facts)))openWith.close();
        if(previewOpen || galleryMode)schedulePreview();
    }
    onGalleryModeChanged:schedulePreview()
    onPreviewOpenChanged:schedulePreview()
    Timer {id:previewDelay;interval:140;onTriggered:root.service.requestPreview(root.current?root.current.path:"")}
    Connections {target:root.service
        function onCapturePresentation(){root.captureHistory();}
        function onHistoryReady(saved){root.restorePresentation(saved);}
        function onInformationReady(details){root.information=details;infoWindow.show(details);}
        function onApplicationsReady(apps,path,selection){if(!root.current || root.current.path!==path || JSON.stringify(root.current.actionFacts)!==JSON.stringify(selection.facts)){root.service.act("retire-actions",{capabilities:[selection.capability]});return;}root.applications=apps;root.applicationsPath=path;root.applicationsSelection=selection;openWith.open();}
        function onSelectedPath(path){
            if(path==="@history"){root.restoringTabId="";}
            else if(root.restoringTabId===root.activeTabId){
                const tab=root.tabs[root.activeTab];root.restoringTabId="";
                root.selectedPaths=(tab.selectedPaths || []).filter(value=>root.service.rows.some(row=>row.path===value));
                root.selected=root.service.rows.findIndex(row=>row.path===tab.selectedPath);
                if(root.selected<0)root.selected=root.service.rows.findIndex(row=>root.selectedPaths.includes(row.path));
                root.selectionAnchor=root.selected;
            }
            else if(path==="@preserve"){root.selectedPaths=root.selectedPaths.filter(p=>root.service.rows.some(r=>r.path===p));}
            else {root.selectedPaths=path?[path]:[];root.selected=root.service.rows.findIndex(r=>r.path===path);}
            root.saveTab();
        }
    }
    Connections {target:content.Window.window;function onActiveChanged(){if(content.Window.window.active)root.shell.activeFiles(root);}}
    Rectangle {id:windowMask;width:root.width;height:root.height;radius:Theme.nativeWindowFrame?0:12;layer.enabled:!Theme.nativeWindowFrame && GraphicsInfo.api !== GraphicsInfo.Software;visible:false}
    Item {
        layer.enabled:!Theme.nativeWindowFrame && GraphicsInfo.api !== GraphicsInfo.Software
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
            else if(ctrl && event.key===Qt.Key_K)networkPopup.open();
            else if(ctrl && event.key===Qt.Key_L)root.dialogFor("goto");
            else if(ctrl && event.key===Qt.Key_F)filter.forceActiveFocus();
            else if(ctrl && shift && event.key===Qt.Key_N)root.dialogFor("mkdir");
            else if(ctrl && shift && (event.key===Qt.Key_Period || event.key===Qt.Key_Greater)){root.service.hidden=!root.service.hidden;root.service.refresh();}
            else if(ctrl && event.key===Qt.Key_C)root.stage("copy");
            else if(ctrl && event.key===Qt.Key_X)root.stage("move");
            else if(ctrl && event.key===Qt.Key_V)root.service.paste();
            else if(ctrl && event.key===Qt.Key_1)root.service.viewMode="grid";
            else if(ctrl && event.key===Qt.Key_D)root.duplicateSelected();
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
        ScrollView {id:favoritesScroll;x:12;y:66;width:root.sidebarWidth-24;height:parent.height-y-14;clip:true;ScrollBar.vertical:DesktopScrollBar {}
          Column {width:favoritesScroll.availableWidth;spacing:3
            SidebarGroup {objectName:"files-sidebar-favourites";width:parent.width;title:"Favourites";collapsed:root.groupCollapsed("favourites");onCollapseRequested:value=>root.collapseSidebar("favourites",value)
              accessory:Component {AbstractButton {width:25;height:23;Accessible.name:"Add current folder to Sidebar";enabled:root.service.path!=="trash:" && !root.sidebarFavorites.some(item=>item.path===root.service.path) && root.sidebarFavorites.length<32;onClicked:root.addSidebar();contentItem:PanelLabel {text:"+";horizontalAlignment:Text.AlignHCenter}background:Rectangle {radius:5;color:parent.hovered?Theme.colors.hover:"transparent"}}}
            Repeater {
                model:root.sidebarFavorites
                AbstractButton {
                    required property var modelData
                    required property int index
                    width:parent.width;height:Math.max(32,Theme.sidebarIconSize+12);enabled:!root.service.busy
                    Accessible.name:modelData.name
                    Keys.onPressed:event=>{if(event.key===Qt.Key_Menu || (event.key===Qt.Key_F10 && (event.modifiers & Qt.ShiftModifier))){root.sidebarEditIndex=index;sidebarActions.x=12;sidebarActions.y=Math.max(40,Math.min(root.height-sidebarActions.height-12,mapToItem(content,0,0).y));sidebarActions.open();event.accepted=true;}else event.accepted=false;}
                    onClicked:root.navigate(modelData.path)
                    background:Rectangle {radius:6;color:root.service.path===parent.modelData.path?Theme.selection:parent.hovered || parent.activeFocus?Theme.colors.hover:"transparent"}
                    contentItem:Item {
                        SystemGlyph {x:8;anchors.verticalCenter:parent.verticalCenter;width:Theme.sidebarIconSize;height:width;name:modelData.path===Quickshell.env("HOME")?"home":"folder";ink:Theme.colors.accent}
                        PanelLabel {x:Theme.sidebarIconSize+16;width:parent.width-x-2;anchors.verticalCenter:parent.verticalCenter;text:modelData.name;textFormat:Text.PlainText;elide:Text.ElideRight}
                        MouseArea {anchors.fill:parent;acceptedButtons:Qt.RightButton;onClicked:event=>{root.sidebarEditIndex=index;sidebarActions.x=12;sidebarActions.y=Math.max(40,Math.min(root.height-sidebarActions.height-12,mapToItem(content,0,0).y));sidebarActions.open();}}
                    }
                }
            }
            }
            Item {width:1;height:12}
            SidebarGroup {objectName:"files-sidebar-locations";width:parent.width;title:"Locations";collapsed:root.groupCollapsed("locations");onCollapseRequested:value=>root.collapseSidebar("locations",value)
            ControlRow {width:parent.width;text:"Network…";onClicked:networkPopup.open()}
            ControlRow {width:parent.width;text:"Computer";enabled:!root.service.busy;onClicked:root.navigate("/")}
            ControlRow {width:parent.width;text:"Trash";selected:root.service.path==="trash:";onClicked:root.navigate("trash:")}
            Repeater {model:root.service.volumes
                Row {required property var modelData;width:parent.width
                    ControlRow {width:parent.width-26;text:modelData.name;onClicked:root.navigate(modelData.path)}
                    ControlRow {width:26;text:"⏏";enabled:modelData.eject;Accessible.name:"Eject "+modelData.name;onClicked:root.service.act("eject",{path:modelData.path})}
                }
            }
            }
          }
        }
        SidebarResizeHandle {id:sidebarResize;objectName:"files-sidebar-resize";x:root.sidebarWidth-4;y:57;height:parent.height-y;value:root.sidebarWidth;maximum:root.sidebarMaximum
            onPreviewRequested:value=>root.sidebarDragWidth=value
            onCommitted:value=>{root.service.sidebarWidth=value;root.sidebarDragWidth=-1;}
            onCancelled:root.sidebarDragWidth=-1
        }
        Row {
            x:root.sidebarWidth+10;y:13;spacing:3
            Repeater {
                model:[{icon:"chevron-left",label:"Back",action:"back"},{icon:"chevron-right",label:"Forward",action:"forward"}]
                AbstractButton {
                    required property var modelData;width:27;height:30
                    enabled:!root.service.busy && (modelData.action==="back"?root.service.historyIndex>0:root.service.historyIndex<root.service.history.length-1)
                    Accessible.name:modelData.label
                    Accessible.description:modelData.action==="preview" && !root.previewLimits.enabled?"Widen this window to show the preview pane":""
                    ToolTip.visible:hovered && modelData.action==="preview" && !root.previewLimits.enabled
                    ToolTip.text:"Widen this window to show the preview pane"
                    onClicked:root.service.back(modelData.action==="back"?-1:1)
                    background:Rectangle {radius:6;color:parent.hovered || parent.activeFocus?Theme.colors.hover:"transparent"}
                    contentItem:Item {SystemGlyph {anchors.centerIn:parent;width:17;height:17;name:modelData.icon;opacity:parent.parent.enabled?1:0.3}}
                }
            }
        }
        PanelLabel {x:root.sidebarWidth+73;y:14;width:Math.max(0,parent.width-x-195);height:29;verticalAlignment:Text.AlignVCenter;text:root.service.path==="trash:"?"Trash":root.service.path.split("/").filter(Boolean).pop() || "Computer";textFormat:Text.PlainText;font.pixelSize:Theme.textRole("subheading");font.weight:Font.DemiBold}
        Row {
            anchors.right:parent.right;anchors.rightMargin:14;y:13;spacing:4
            DesktopComboBox {width:112+(Theme.textRole("body")-13)*5;height:Math.max(30,Theme.textRole("body")+14);model:["Icons","List","Columns","Gallery"];currentIndex:["grid","list","columns","gallery"].indexOf(root.service.viewMode);Accessible.name:"Files view";onActivated:index=>root.service.viewMode=["grid","list","columns","gallery"][index]}
            Repeater {
                model:[{icon:"info",label:"Preview",action:"preview"},{icon:"more",label:"File actions",action:"menu"}]
                AbstractButton {
                    required property var modelData;width:30;height:30
                    Accessible.name:modelData.label
                    Accessible.description:modelData.action==="preview" && !root.previewLimits.enabled?"Widen this window to show the preview pane":""
                    ToolTip.visible:hovered && modelData.action==="preview" && !root.previewLimits.enabled
                    ToolTip.text:"Widen this window to show the preview pane"
                    onClicked:{if(modelData.action==="grid")root.service.viewMode="grid";else if(modelData.action==="list")root.service.viewMode="list";else if(modelData.action==="preview")root.previewOpen=!root.previewOpen;else {actions.x=root.width-230;actions.y=48;actions.open();}}
                    background:Rectangle {radius:7;color:parent.hovered || parent.activeFocus || (parent.modelData.action==="list" && root.listMode) || (parent.modelData.action==="grid" && !root.listMode) || (parent.modelData.action==="preview" && root.previewOpen)?Theme.selection:"transparent"}
                    contentItem:Item {SystemGlyph {anchors.centerIn:parent;width:17;height:17;name:modelData.icon}}
                }
            }
        }
        DocumentTabs {id:documentTabs;objectName:"files-tabs";x:root.sidebarWidth+10;y:58;width:parent.width-x-10;height:root.tabHeight;visible:root.tabs.length>1;tabs:root.tabs;activeId:root.activeTabId
            onSelectRequested:id=>root.switchTab(TabModel.indexOf(root.tabs,id))
            onCloseRequested:id=>root.closeTabId(id)
            onMoveRequested:(id,target)=>root.reorderTab(id,target)
            onNewRequested:root.newTab()
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
            PanelLabel {width:parent.width-78;anchors.verticalCenter:parent.verticalCenter;font.pixelSize:Theme.textRole("metadata");textFormat:Text.PlainText
                text:root.service.currentJob?root.service.currentJob.operation+" · "+root.service.currentJob.completed+"/"+root.service.currentJob.total+" · "+root.service.currentJob.state+(root.service.working?" · "+(root.service.currentJob.bytes/1048576).toFixed(1)+" MB":""):""
                Accessible.name:text
            }
            DialogButton {width:70;height:26;visible:root.service.working;text:"Cancel";onClicked:root.service.cancelJob()}
        }
        Popup {id:sidebarActions;width:175;height:124;modal:true;focus:true;padding:6
            background:Rectangle {radius:8;color:Theme.colors.background;border.color:Theme.outline}
            contentItem:Column {spacing:2
                ControlRow {width:parent.width;text:"Move Up";enabled:root.sidebarEditIndex>0;onClicked:root.editSidebar("up")}
                ControlRow {width:parent.width;text:"Move Down";enabled:root.sidebarEditIndex>=0 && root.sidebarEditIndex<root.sidebarFavorites.length-1;onClicked:root.editSidebar("down")}
                ControlRow {width:parent.width;text:"Remove from Sidebar";onClicked:root.editSidebar("remove")}
            }
        }
        Popup {id:tagsEditor;width:Math.min(330,root.width-40);height:Math.min(430,root.height-40);x:(root.width-width)/2;y:(root.height-height)/2;modal:true;focus:true;padding:16
            background:Rectangle {radius:12;color:Theme.colors.background;border.color:Theme.outline}
            contentItem:ScrollView {id:tagsEditorScroll;clip:true;ScrollBar.vertical:DesktopScrollBar {}
              Column {width:tagsEditorScroll.availableWidth;spacing:7
                PanelLabel {text:"Colour Tags";font.weight:Font.DemiBold}
                PanelLabel {width:parent.width;wrapMode:Text.WordWrap;text:root.tagsError?root.tagsError:"Stored in file metadata; filter with tag:red or another colour. Links are not followed.";font.pixelSize:Theme.textRole("metadata");color:Theme.colors.muted}
                Repeater {model:root.colourTags
                    CheckBox {required property var modelData;text:modelData.label;checked:root.selectedTags.includes(modelData.id);enabled:root.tagsSupported;palette.windowText:Theme.colors.text
                        onToggled:root.selectedTags=checked?root.selectedTags.concat([modelData.id]):root.selectedTags.filter(tag=>tag!==modelData.id)
                        indicator:Rectangle {width:16;height:16;radius:8;color:parent.modelData.color;border.color:parent.checked?Theme.colors.text:"transparent";border.width:2}
                        contentItem:PanelLabel {text:parent.text;leftPadding:24;verticalAlignment:Text.AlignVCenter}
                    }
                }
                Row {spacing:8
                    DialogButton {width:100;text:"Cancel";onClicked:tagsEditor.close()}
                    DialogButton {width:100;text:"Apply";primary:true;enabled:root.tagsSupported;onClicked:{tagsEditor.close();root.service.act("set-tags",{path:root.tagsTarget,identity:root.tagsIdentity,tags:root.selectedTags});}}
                }
            } }
        }
        Popup {id:viewOptions;width:Math.min(340,root.width-40);height:Math.min(380,root.height-40);x:(root.width-width)/2;y:(root.height-height)/2;modal:true;focus:true;padding:16
            background:Rectangle {radius:12;color:Theme.colors.background;border.color:Theme.outline}
            contentItem:ScrollView {id:viewOptionsScroll;clip:true;ScrollBar.vertical:DesktopScrollBar {}
              Column {width:viewOptionsScroll.availableWidth;spacing:10
                PanelLabel {text:"View Options";font.weight:Font.DemiBold}
                PanelLabel {width:parent.width;text:"Preferences are saved as defaults for Files windows.";wrapMode:Text.WordWrap;font.pixelSize:Theme.textRole("metadata");color:Theme.colors.muted}
                DesktopComboBox {width:parent.width;model:["Name","Modified","Size","Kind"];currentIndex:["name","modified","size","kind"].indexOf(root.service.sort);Accessible.name:"Sort files";onActivated:index=>{root.service.sort=["name","modified","size","kind"][index];root.service.refresh();}}
                SettingsToggle {width:parent.width;text:"Descending order";checked:root.service.descending;onClicked:{root.service.descending=checked;root.service.refresh();}}
                SettingsToggle {width:parent.width;text:"Keep folders on top";checked:root.service.foldersFirst;onClicked:{root.service.foldersFirst=checked;root.service.refresh();}}
                DesktopComboBox {width:parent.width;model:["No groups","Group by kind (list view)"];currentIndex:root.service.groupBy==="kind"?1:0;Accessible.name:"Group files";onActivated:index=>{root.service.groupBy=index===1?"kind":"none";if(index===1)root.service.viewMode="list";root.service.refresh();}}
                DialogButton {width:100;text:"Done";onClicked:viewOptions.close()}
            } }
        }
        Popup {id:openWith;onClosed:{if(root.applicationsSelection)root.service.act("retire-actions",{capabilities:[root.applicationsSelection.capability]});root.applicationsSelection=null;}width:Math.min(360,root.width-40);height:Math.min(380,root.height-80);x:(root.width-width)/2;y:(root.height-height)/2;modal:true;focus:true;padding:12
            background:Rectangle {radius:12;color:Theme.colors.background;border.color:Theme.outline}
            contentItem:ListView {clip:true;model:root.applications;ScrollBar.vertical:DesktopScrollBar {}
                delegate:Row {required property var modelData;width:ListView.view.width;height:38
                    CapturedActionButton {width:parent.width-100;text:modelData.name+(modelData.default?" (default)":"");selection:root.applicationsSelection?Object.assign({},root.applicationsSelection,{application:modelData.id}):null;onActivatedIntent:captured=>{root.applicationsSelection=null;openWith.close();root.service.act("open-with",{capability:captured.capability,choice:captured.choice,application:captured.application});}}
                    CapturedActionButton {width:100;height:30;text:"Set Default";selection:root.applicationsSelection?Object.assign({},root.applicationsSelection,{application:modelData.id}):null;enabled:!modelData.default;Accessible.name:"Use "+modelData.name+" for this file type";onActivatedIntent:captured=>{root.applicationsSelection=null;openWith.close();root.service.act("set-default",{capability:captured.capability,choice:captured.choice,application:captured.application});}}
                }
                PanelLabel {visible:root.applications.length===0;text:"No applications registered for this file";width:parent.width;wrapMode:Text.WordWrap}
            }
        }
        TextField {
            placeholderTextColor:Theme.colors.muted;selectionColor:Theme.colors.accent;selectedTextColor:Theme.colors.selectedText

            id:filter;x:root.sidebarWidth+14;y:66+root.tabHeight;width:parent.width-x-14;height:30
            placeholderText:"Filter folder or tag:red";font.family:Theme.font;font.pixelSize:Theme.textRole("label");color:Theme.colors.text;selectByMouse:true
            Accessible.name:"Filter this folder"
            background:Rectangle {radius:7;color:Theme.colors.background;border.color:filter.activeFocus?Theme.colors.accent:Theme.hairline}
            onTextEdited:filterDelay.restart()
            Keys.onDownPressed:{content.forceActiveFocus();root.step(1);}
            onAccepted:{root.service.query=text;root.service.refresh();content.forceActiveFocus();}
        }
        Timer {id:filterDelay;interval:220;onTriggered:{root.service.query=filter.text;root.service.refresh();}}
        PanelLabel {
            id:message;x:root.sidebarWidth+14;y:103+root.tabHeight;width:parent.width-x-14;height:visible?Math.max(48,Math.ceil(messageMetrics.height)*3):0
            visible:root.service.error!=="" || root.service.warning!=="" || root.previewMessage!==""
            text:root.service.error || root.service.warning || root.previewMessage;wrapMode:Text.WrapAnywhere;textFormat:Text.PlainText;font.pixelSize:Theme.textRole("label");color:Theme.colors.muted;elide:Text.ElideRight;maximumLineCount:3
            MouseArea {anchors.fill:parent;enabled:root.service.error!=="" || root.service.warning!=="";onClicked:errorDetails.open()}
            FontMetrics {id:messageMetrics;font.family:Theme.font;font.pixelSize:Theme.textRole("label")}
            Accessible.name:(root.service.error || root.service.warning?"Operation details: ":"")+text
            Accessible.onPressAction:if(root.service.error || root.service.warning)errorDetails.open()
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
            width:parent.width-x-(root.previewVisible?22+root.previewWidth:10)
            height:parent.height-y-64-(root.footerHeight-20)-(Theme.textRole("body")-13)-(root.service.currentJob?40:0)
            enabled:!root.service.busy
            Row {
                id:headings;visible:root.listMode;width:Math.max(0,parent.width-root.selectionGutter-(listScrollBar.visible?listScrollBar.width:0));height:Math.max(26,Theme.textRole("body")+12)
                Repeater {
                    model:root.compactMetadata?[{key:"name",label:"Name",fraction:1}]:[{key:"name",label:"Name",fraction:0.55},{key:"modified",label:"Modified",fraction:0.27},{key:"size",label:"Size",fraction:0.18}]
                    ControlRow {required property var modelData;width:headings.width*modelData.fraction;height:Math.max(26,implicitHeight);topPadding:3;bottomPadding:3;text:modelData.label+(root.service.sort===modelData.key?(root.service.descending?" ↓":" ↑"):"");contentItem:PanelLabel {text:parent.text;elide:Text.ElideRight;verticalAlignment:Text.AlignVCenter}onClicked:root.sortBy(modelData.key)}
                }
            }
            ListView {
                section.property:root.service.groupBy==="kind"?"modelData.group":""
                section.delegate:PanelLabel {required property string section;width:list.width;height:30;text:section;font.weight:Font.DemiBold;topPadding:8;leftPadding:8}
                id:list;objectName:"files-list-view";visible:root.listMode;anchors.top:headings.bottom;anchors.bottom:parent.bottom;width:parent.width;clip:true;model:root.service.rows
                ScrollBar.vertical:DesktopScrollBar {id:listScrollBar}
                delegate:Rectangle {
                    required property var modelData;required property int index
                    width:Math.max(0,list.width-root.selectionGutter-(listScrollBar.visible?listScrollBar.width:0));height:root.labelRowHeight;radius:5;color:root.isSelected(index)?root.selectionFill:index%2?Theme.colors.background:"transparent"
                    SystemGlyph {x:8;anchors.verticalCenter:parent.verticalCenter;name:modelData.directory?"folder":"document";width:18;height:18;ink:root.isSelected(index)?root.selectionText:Theme.colors.accent}
                    PanelLabel {x:34;anchors.verticalCenter:parent.verticalCenter;width:parent.width*(root.compactMetadata?1:.55)-40;text:FileLinks.markedLabel(modelData);textFormat:Text.PlainText;color:root.isSelected(index)?root.selectionText:Theme.colors.text;font.pixelSize:Theme.textRole("label")}
                    PanelLabel {visible:!root.compactMetadata;x:parent.width*.55+8;anchors.verticalCenter:parent.verticalCenter;width:parent.width*.27-10;text:Qt.formatDateTime(new Date(modelData.mtime*1000),"dd MMM yyyy");font.pixelSize:Theme.textRole("metadata");color:root.isSelected(index)?root.selectionText:Theme.colors.muted}
                    PanelLabel {visible:!root.compactMetadata;x:parent.width*.82+8;anchors.verticalCenter:parent.verticalCenter;width:parent.width*.18-12;text:root.sizeText(modelData);font.pixelSize:Theme.textRole("metadata");color:root.isSelected(index)?root.selectionText:Theme.colors.muted}
                    FolderDropTarget {anchors.fill:parent;enabled:modelData.directory && root.service.path!=="trash:";folder:modelData.path;coordinator:fileDrop}
                    MouseArea {anchors.fill:parent;acceptedButtons:Qt.LeftButton|Qt.RightButton;drag.target:dragGhost
                        onPressed:event=>{if(event.button===Qt.LeftButton && !event.modifiers && !root.isSelected(index))root.choose(index,event.modifiers);}
                        onPositionChanged:if(drag.active && !root.dragging){root.dragUris=root.selectedUris();Qt.callLater(root.beginDrag);}
                        onClicked:event=>{if(event.button!==Qt.RightButton || !root.isSelected(index))root.choose(index,event.modifiers);if(event.button===Qt.RightButton){actions.x=Math.min(root.width-actions.width-8,mapToItem(content,event.x,event.y).x);actions.y=Math.min(root.height-actions.height-8,mapToItem(content,event.x,event.y).y);actions.open();}} onDoubleClicked:root.openCurrent()}
                    Accessible.name:FileLinks.accessible(modelData);Accessible.role:Accessible.ListItem;Accessible.selected:root.isSelected(index);Accessible.onPressAction:root.choose(index,0)
                }
            }
            GridView {
                id:grid;objectName:"files-grid-view";visible:root.service.viewMode==="grid";anchors.fill:parent;cellWidth:108+(Theme.textRole("label")-12)*4;cellHeight:108+(Theme.textRole("label")-12)*3;clip:true;model:root.service.rows
                ScrollBar.vertical:DesktopScrollBar {id:gridScrollBar}
                delegate:Item {
                    required property var modelData;required property int index
                    width:Math.max(1,grid.cellWidth-root.selectionGutter-(gridScrollBar.visible?gridScrollBar.width:0));height:grid.cellHeight
                    Accessible.name:FileLinks.accessible(modelData);Accessible.role:Accessible.ListItem;Accessible.selected:root.isSelected(index);Accessible.onPressAction:root.choose(index,0)
                    Rectangle {x:(parent.width-68)/2;y:4;width:68;height:62;radius:8;color:root.isSelected(index)?Theme.selection:"transparent"}
                    Image {visible:modelData.directory;x:(parent.width-58)/2;y:9;width:58;height:48;source:"icons/folder.svg";sourceSize:Qt.size(116,96)}
                    Image {id:thumbnail;property bool failed:false;onStatusChanged:if(status===Image.Error)failed=true;visible:!modelData.directory && ["image/png","image/jpeg","image/webp","image/gif","image/bmp"].includes(modelData.kind) && modelData.size<=33554432 && !failed;x:(parent.width-60)/2;y:8;width:60;height:54;source:visible?modelData.uri:"";sourceSize:Qt.size(120,108);fillMode:Image.PreserveAspectFit;asynchronous:true}
                    SystemGlyph {visible:!modelData.directory && !thumbnail.visible;x:(parent.width-36)/2;y:14;width:36;height:40;name:"document";ink:Theme.colors.muted}
                    PanelLabel {visible:modelData.link;x:22;y:45;text:FileLinks.badge(modelData);font.pixelSize:16}
                    Rectangle {x:3;y:70;width:parent.width-6;height:30+(Theme.textRole("label")-12)*3;radius:4;color:root.isSelected(index)?root.selectionFill:"transparent"}
                    PanelLabel {x:5;y:70;width:parent.width-10;height:30+(Theme.textRole("label")-12)*3;text:FileLinks.label(modelData);textFormat:Text.PlainText;horizontalAlignment:Text.AlignHCenter;verticalAlignment:Text.AlignVCenter;wrapMode:Text.Wrap;maximumLineCount:2;font.pixelSize:Theme.textRole("label");color:root.isSelected(index)?root.selectionText:Theme.colors.text}
                    FolderDropTarget {anchors.fill:parent;enabled:modelData.directory && root.service.path!=="trash:";folder:modelData.path;coordinator:fileDrop}
                    MouseArea {anchors.fill:parent;acceptedButtons:Qt.LeftButton|Qt.RightButton;drag.target:dragGhost
                        onPressed:event=>{if(event.button===Qt.LeftButton && !event.modifiers && !root.isSelected(index))root.choose(index,event.modifiers);}
                        onPositionChanged:if(drag.active && !root.dragging){root.dragUris=root.selectedUris();Qt.callLater(root.beginDrag);}
                        onClicked:event=>{if(event.button!==Qt.RightButton || !root.isSelected(index))root.choose(index,event.modifiers);if(event.button===Qt.RightButton){actions.x=Math.min(root.width-actions.width-8,mapToItem(content,event.x,event.y).x);actions.y=Math.min(root.height-actions.height-8,mapToItem(content,event.x,event.y).y);actions.open();}} onDoubleClicked:root.openCurrent()}
                }
            }
            FilesBandSelection {objectName:"files-list-band-selection";parent:list;anchors.fill:parent;view:list;rows:root.service.rows;selectedPaths:root.selectedPaths;currentIndex:root.selected;anchorIndex:root.selectionAnchor;contextKey:root.bandContext;permitted:root.bandPermitted && list.visible;rightInset:listScrollBar.visible?listScrollBar.width:0;onBegan:root.cancelPendingPresentation();onPreviewed:paths=>root.bandPreviewPaths=paths;onCommitted:value=>root.commitBand(value);onCanceled:reason=>root.cancelBand(reason)}
            FilesBandSelection {objectName:"files-grid-band-selection";parent:grid;anchors.fill:parent;view:grid;rows:root.service.rows;selectedPaths:root.selectedPaths;currentIndex:root.selected;anchorIndex:root.selectionAnchor;contextKey:root.bandContext;permitted:root.bandPermitted && grid.visible;rightInset:gridScrollBar.visible?gridScrollBar.width:0;onBegan:root.cancelPendingPresentation();onPreviewed:paths=>root.bandPreviewPaths=paths;onCommitted:value=>root.commitBand(value);onCanceled:reason=>root.cancelBand(reason)}
            ColumnView {id:columnView;objectName:"files-column-view";preferredWidths:root.service.columnWidths || [];onWidthCommitted:(index,value)=>{const widths=(root.service.columnWidths || []).slice();while(widths.length<=index)widths.push(240);widths[index]=value;root.service.columnWidths=widths;};selectedPaths:root.bandPreviewPaths===null?root.selectedPaths:root.bandPreviewPaths;bandBasePaths:root.selectedPaths;bandRows:root.service.rows;bandPermitted:root.bandPermitted;bandContext:root.bandContext;bandCurrent:root.selected;bandAnchor:root.selectionAnchor;selectionFolder:root.service.path;onBandBegan:root.cancelPendingPresentation();onBandPreviewed:paths=>root.bandPreviewPaths=paths;onBandCommitted:value=>root.commitBand(value);onBandCanceled:reason=>root.cancelBand(reason);selectionActive:root.selectionActive;dropCoordinator:fileDrop;visible:root.columnsMode;anchors.fill:parent;columns:root.service.columns;currentPath:root.current?root.current.path:"";onChosen:(path,directory)=>{if(directory)root.navigate(path);else {const index=root.service.rows.findIndex(r=>r.path===path);if(index>=0)root.choose(index,0);else {root.service.nextSelection=path;root.navigate(path.slice(0,path.lastIndexOf("/")) || "/");}}}}
            Item {visible:root.galleryMode;anchors.fill:parent
                FilePreview {resourceRetirement:root.service.previewResourceRetirement;x:12;y:0;width:parent.width-24;height:Math.max(60,parent.height-118);result:root.current?root.service.preview:{text:"Select an item"};active:root.visible && root.galleryMode;loading:root.service.previewBusy;onPageRequested:number=>root.service.requestPreview(root.current.path,number)}
                ListView {id:galleryStrip;objectName:"files-gallery-view";anchors.bottom:parent.bottom;width:parent.width;height:104+(Theme.textRole("metadata")-11)*3;orientation:ListView.Horizontal;clip:true;spacing:5;model:root.service.rows;ScrollBar.horizontal:DesktopScrollBar {id:galleryScrollBar}
                    delegate:AbstractButton {required property var modelData;required property int index;width:92;height:Math.max(1,galleryStrip.height-root.selectionGutter-(galleryScrollBar.visible?galleryScrollBar.height:0));
                        FolderDropTarget {anchors.fill:parent;enabled:modelData.directory && root.service.path!=="trash:";folder:modelData.path;coordinator:fileDrop}
Accessible.name:FileLinks.accessible(modelData);Accessible.selected:root.isSelected(index);onClicked:root.choose(index,0);onDoubleClicked:root.openCurrent()
                        background:Rectangle {radius:7;color:root.isSelected(index)?Theme.selection:parent.hovered?Theme.colors.hover:"transparent";border.color:parent.activeFocus?Theme.colors.accent:"transparent"}
                        contentItem:Item {
                            Image {id:galleryThumb;anchors.horizontalCenter:parent.horizontalCenter;y:4;width:72;height:46;source:modelData.directory?"icons/folder.svg":(["image/png","image/jpeg","image/webp","image/gif","image/bmp"].includes(modelData.kind) && modelData.size<=33554432?modelData.uri:"");sourceSize:Qt.size(144,92);fillMode:Image.PreserveAspectFit;asynchronous:true}
                            SystemGlyph {visible:!galleryThumb.source || galleryThumb.status===Image.Error;anchors.horizontalCenter:parent.horizontalCenter;y:5;width:36;height:42;name:"document";ink:Theme.colors.muted}
                            PanelLabel {y:52;width:parent.width;height:Math.max(0,parent.height-52);text:FileLinks.markedLabel(modelData);textFormat:Text.PlainText;wrapMode:Text.Wrap;maximumLineCount:2;horizontalAlignment:Text.AlignHCenter;font.pixelSize:Theme.textRole("metadata")}}
                    }
                }
            }
            FilesBandSelection {objectName:"files-gallery-band-selection";parent:galleryStrip;anchors.fill:parent;view:galleryStrip;rows:root.service.rows;selectedPaths:root.selectedPaths;currentIndex:root.selected;anchorIndex:root.selectionAnchor;contextKey:root.bandContext;permitted:root.bandPermitted && root.galleryMode;bottomInset:galleryScrollBar.visible?galleryScrollBar.height:0;onBegan:root.cancelPendingPresentation();onPreviewed:paths=>root.bandPreviewPaths=paths;onCommitted:value=>root.commitBand(value);onCanceled:reason=>root.cancelBand(reason)}
            PanelLabel {anchors.centerIn:parent;visible:root.service.rows.length===0;text:root.service.busy?"Loading…":root.service.query?"No matching items":"This folder is empty";color:Theme.colors.muted}
        }
        Item {
            id:previewPane;visible:root.previewVisible;x:files.x+files.width+8;y:files.y;width:parent.width-x-14;height:files.height
            Rectangle {width:1;height:parent.height;color:Theme.hairline}
            Column {
                x:12;width:parent.width-12;spacing:10
                PanelLabel {width:parent.width;text:root.current?root.current.name:"Select an item";textFormat:Text.PlainText;wrapMode:Text.WrapAnywhere;maximumLineCount:3;font.weight:Font.DemiBold}
                PanelLabel {width:parent.width;text:root.current?(root.current.directory?"Folder":root.sizeText(root.current)):"";color:Theme.colors.muted;font.pixelSize:Theme.textRole("metadata")}
                PreviewResourceConsumer {manager:root.service.previewResourceRetirement;sourceObjects:[sidebarPreviewImage]}
                Image {id:sidebarPreviewImage;visible:root.service.preview.type==="image";width:parent.width;height:visible?Math.max(80,previewPane.height-105):0;source:visible?root.service.preview.uri || "":"";sourceSize:Qt.size(800,800);fillMode:Image.PreserveAspectFit;asynchronous:true;PanelLabel {visible:parent.status===Image.Error;anchors.centerIn:parent;text:"Preview unavailable";color:Theme.colors.muted}}
                Flickable {
                    visible:root.service.preview.type!=="image";width:parent.width;height:Math.max(40,previewPane.height-110);clip:true;contentHeight:previewText.height;contentWidth:width
                    PanelLabel {id:previewText;width:parent.width;text:root.current?root.service.preview.text || root.service.preview.error || (root.service.previewBusy?"Loading preview…":"Preview unavailable"):"";textFormat:Text.PlainText;wrapMode:Text.WrapAnywhere;elide:Text.ElideNone;font.pixelSize:Theme.textRole("label")}
                }
                PanelLabel {visible:!!root.service.preview.truncated;text:"Preview shortened";font.pixelSize:Theme.textRole("caption");color:Theme.colors.muted}
            }
        }
        SplitResizeHandle {objectName:"files-preview-resize";label:"Preview width";direction:-1;visible:root.previewVisible && root.previewLimits.maximum>root.previewLimits.minimum;x:previewPane.x-4;y:previewPane.y;height:previewPane.height;value:root.previewWidth;minimum:root.previewLimits.minimum;maximum:root.previewLimits.maximum
            onPreviewRequested:value=>root.previewDragWidth=value
            onCommitted:value=>{root.service.previewWidth=value;root.previewDragWidth=-1;}
            onCancelled:root.previewDragWidth=-1
        }
        Flickable {
            id:pathbar;onContentWidthChanged:Qt.callLater(()=>contentX=Math.max(0,contentWidth-width));x:root.sidebarWidth+12;y:parent.height-57-(root.footerHeight-20);width:parent.width-x-12;height:25+(Theme.textRole("body")-13);contentWidth:crumbs.width;clip:true
            Row {
                id:crumbs;spacing:1
                Repeater {
                    model:root.displayCrumbs()
                    ControlRow {required property var modelData;text:modelData.name+"  ›";height:25+(Theme.textRole("body")-13);contentItem:PanelLabel {text:parent.text;verticalAlignment:Text.AlignVCenter;elide:Text.ElideRight}topPadding:3;bottomPadding:3;width:Math.min(220,Math.max(40,labelMetrics.advanceWidth(modelData.name)+46));enabled:!root.service.busy;onClicked:root.navigate(modelData.path)}
                }
            }
        }
        FontMetrics {id:labelMetrics;font.family:Theme.font;font.pixelSize:Theme.textRole("body")}
        PanelLabel {x:root.sidebarWidth+14;y:parent.height-root.footerHeight-8;width:parent.width-x-14;height:root.footerHeight;text:root.service.busy?"Loading…":root.selectedPaths.length?root.selectedPaths.length+" selected · "+root.service.rows.length+" items":root.service.stagedPath?(root.service.stagedAction==="move"?"Move":"Copy")+" ready · Navigate to a destination and Paste":root.service.rows.length+(root.service.rows.length===1?" item":" items")+(root.service.hidden?" · Hidden items shown":"");font.pixelSize:Theme.textRole("metadata");color:Theme.colors.muted}
        TrashConfirmation {state:trashActions}
        TrashResults {id:trashResults;state:trashActions}
        Connections {target:trashActions;function onErrorChanged(){if(trashActions.error)trashResults.open();}function onBusyChanged(){if(trashActions.busy)trashResults.open();}}
        Popup {
            id:actions
            objectName:"files-actions-menu"
            property var sharingIntent:null
            onOpened:{sharingIntent=FileShareIntent.capture({path:root.service.path,tab:root.activeTabId},root.selectedPaths,root.service.rows);NearbySharingState.refresh();}
            onClosed:sharingIntent=null
            focus:true;width:220;height:Math.min(550,root.height-70);padding:6;clip:true
            background:Rectangle {radius:10;color:Theme.colors.background;border.color:Theme.outline}
            contentItem:Flickable {contentHeight:actionColumn.height;clip:true;focus:true
              Keys.onDownPressed:root.stepAction(1)
              Keys.onUpPressed:root.stepAction(-1)
              ScrollBar.vertical:DesktopScrollBar {}
              Column {id:actionColumn;width:208
                Repeater {id:actionRows
                    model:[{label:"Open",action:"open",enabled:!!root.current},{label:"Open With…",action:"open-with",enabled:!!root.current && !root.current.directory},{label:NearbySharingState.label+"…",action:"nearby-send",enabled:NearbySharingState.canSend && root.selectedPaths.length>0 && root.selectedPaths.every(p=>root.service.rows.some(r=>r.path===p && !r.directory)) && root.service.path!=="trash:"},{label:NearbySharingState.label+" Receiving…",action:"nearby-open",enabled:NearbySharingState.available},{separator:true,label:""},{label:"New Tab",action:"new-tab",enabled:true},{label:"New Window",action:"new-window",enabled:true},{label:"New Folder…",action:"mkdir",enabled:root.service.writable},{label:"Rename…",action:"rename",enabled:root.selectedPaths.length>0 && root.service.path!=="trash:" && !root.service.working},{separator:true,label:""},{label:"Copy",action:"copy",enabled:!!root.current},{label:"Cut",action:"move",enabled:!!root.current},{label:"View Options…",action:"view-options",enabled:true},{label:"Colour Tags…",action:"tags",enabled:root.selectedPaths.length===1 && root.service.path!=="trash:"},{label:"Make Symbolic Link",action:"make-link",enabled:root.selectedPaths.length===1 && root.service.path!=="trash:" && root.service.writable},{label:"Duplicate",action:"duplicate",enabled:root.selectedPaths.length>0 && root.service.writable && root.service.path!=="trash:" && !root.service.working},{label:"Paste",action:"paste",enabled:root.service.writable},{label:"Paste Keeping Both",action:"paste-both",enabled:root.service.writable},{label:"Undo Last Move",action:"undo",enabled:root.service.canUndo},{separator:true,label:""},{label:"Put Back",action:"restore",enabled:root.service.path==="trash:" && !!root.current},{label:"Delete Permanently…",action:"permanent-delete",enabled:root.service.path==="trash:" && root.selectedPaths.length>0 && !trashActions.busy},{label:"Empty Trash…",action:"empty-trash",enabled:root.service.path==="trash:" && !trashActions.busy},{label:"Trash Results…",action:"trash-results",enabled:root.service.path==="trash:"},{label:"Move to Trash…",action:"trash",enabled:root.service.path!=="trash:" && !!root.current},{separator:true,label:""},{label:"Go to Folder…",action:"goto",enabled:true},{label:root.service.hidden?"Hide Hidden Files":"Show Hidden Files",action:"hidden",enabled:true},{label:"Refresh",action:"refresh",enabled:true}]
                    ControlRow {
                        objectName:"files-action-"+modelData.action
                        required property var modelData;width:208;height:modelData.separator?9:Math.max(30,implicitHeight);text:modelData.label;Rectangle {visible:!!modelData.separator;x:8;width:parent.width-16;height:1;anchors.verticalCenter:parent.verticalCenter;color:Theme.hairline}enabled:!modelData.separator && modelData.enabled && !root.service.busy
                        onClicked:{
                            const sharing=actions.sharingIntent;
                            const sharingCurrent=FileShareIntent.current(sharing,{path:root.service.path,tab:root.activeTabId},root.selectedPaths,root.service.rows);
                            actions.close();const a=modelData.action;
                            if(a==="open")root.openCurrent();else if(a==="copy" || a==="move")root.stage(a);else if(a==="paste")root.service.paste();
                            else if(a==="paste-both")root.service.paste("keep-both");
                            else if(a==="undo")root.service.undo();
                            else if(a==="new-tab")root.newTab();
                            else if(a==="new-window")root.shell.newFilesWindow(root.service.path);
                            else if(a==="restore")root.service.startJob("restore",root.selectedPaths);
                            else if(a==="permanent-delete")root.prepareDeletion("selected");
                            else if(a==="empty-trash")root.prepareDeletion("empty");
                            else if(a==="trash-results"){trashActions.refresh();trashResults.open();}
                            else if(a==="open-with")root.service.act("applications",{path:root.current.path});
                            else if(a==="nearby-send"){
                                if(sharingCurrent && NearbySharingState.canSend)AppLaunch.run(FileShareIntent.argumentsFor(sharing));
                                else root.service.error="The sharing selection or provider changed. Select the files and open the menu again.";
                            }
                            else if(a==="nearby-open" && NearbySharingState.available)AppLaunch.run(["lumen-nearby-sharing","open"]);
                            else if(a==="duplicate")root.duplicateSelected();
                            else if(a==="make-link")root.service.act("make-link",{path:root.current.path});
                            else if(a==="view-options")viewOptions.open();
                            else if(a==="tags")root.editTags();
                            else if(a==="hidden"){root.service.hidden=!root.service.hidden;root.service.refresh();}
                            else if(a==="refresh")root.service.refresh();else root.dialogFor(a);
                        }
                    }
                }
            }
        }
        }
        Popup {
            id:networkPopup;modal:true;focus:true;width:Math.min(620,root.width-32);height:Math.min(760,root.height-32);x:(root.width-width)/2;y:16;padding:16
            background:Rectangle {radius:14;color:Theme.colors.background;border.color:Theme.outline}
            contentItem:Column {spacing:9
                ScrollView {id:networkScroll;width:parent.width;height:parent.height-42;clip:true;ScrollBar.vertical:DesktopScrollBar {}
                    NetworkLocationsPane {width:networkScroll.availableWidth;service:networkLocations;onOpenLocalFolder:path=>{networkPopup.close();root.navigate(path);}}
                }
                DialogButton {width:parent.width;height:32;text:"Done";onClicked:networkPopup.close()}
            }
        }
        BatchRenameDialog {id:batchRename;service:root.service;ownerWidth:root.width;ownerHeight:root.height}
        Popup {
            id:editor;property string action:"";property string target:"";property string label:""
            modal:true;focus:true;width:Math.min(430,root.width-40);x:(root.width-width)/2;y:Math.max(30,(root.height-height)/2);padding:20
            background:Rectangle {radius:14;color:Theme.colors.background;border.color:Theme.outline}
            contentItem:Column {
                spacing:14
                PanelLabel {width:parent.width;text:editor.action==="trash"?"Move to Trash?":editor.action==="rename"?"Rename":editor.action==="goto"?"Go to Folder":"New Folder";font.pixelSize:Theme.textRole("heading");font.weight:Font.DemiBold}
                PanelLabel {visible:editor.action==="trash";width:parent.width;text:editor.label+" will be moved to Trash.";textFormat:Text.PlainText;wrapMode:Text.WrapAnywhere}
                TextField {
            placeholderTextColor:Theme.colors.muted;selectionColor:Theme.colors.accent;selectedTextColor:Theme.colors.selectedText
id:entry;visible:editor.action!=="trash";width:parent.width;height:Math.max(34,Theme.textRole("body")+16);font.pixelSize:Theme.textRole("body");color:Theme.colors.text;font.family:Theme.font;selectByMouse:true;placeholderText:editor.action==="goto"?"Folder path":"Name";Accessible.name:placeholderText;background:Rectangle{radius:6;color:Theme.colors.field;border.color:Theme.colors.accent} onAccepted:if(text)root.submit()}
                Row {
                    spacing:8
                    DialogButton {id:cancelEditor;width:110;text:"Cancel";defaultAction:editor.action==="trash";onClicked:editor.close()}
                    DialogButton {width:160;destructive:editor.action==="trash";primary:editor.action!=="trash";text:editor.action==="trash"?"Move to Trash":editor.action==="goto"?"Go":editor.action==="mkdir"?"Create":"Rename";enabled:editor.action==="trash" || entry.text.length>0;onClicked:root.submit()}
                }
            }
        }
    }
}

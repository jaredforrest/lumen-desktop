import QtQuick
import QtQuick.Controls
import Quickshell
import Quickshell.Wayland
import "MenuLayout.js" as MenuLayout
import "ClockFormat.js" as ClockFormat
import "MenuVisibilityPolicy.js" as MenuVisibilityPolicy

PanelWindow {
    id: bar
    property var exported: shell.exportedMenu
    property string outputName:""
    readonly property bool outputFullscreen:!!shell.menuVisibility && shell.menuVisibility.available && shell.menuVisibility.outputs[outputName]===true
    readonly property bool automaticHide:MenuVisibilityPolicy.automatic(Theme.menuBarPreferences,outputFullscreen)
    readonly property bool automaticFullscreen:outputFullscreen && automaticHide
    property bool edgeRevealed:false
    readonly property bool chromeShown:!automaticHide || edgeRevealed || menu!==""
    visible:chromeShown
    onAutomaticHideChanged:{edgeRevealed=false;revealHide.stop();}
    HoverHandler {id:barHover;onHoveredChanged:{if(hovered)revealHide.stop();else revealHide.restart();}}
    Timer {id:revealHide;interval:500;onTriggered:if(bar.automaticHide && !barHover.hovered && bar.menu==="")bar.edgeRevealed=false}
    Connections {target:bar;function onMenuChanged(){if(bar.menu!=="")revealHide.stop();else if(bar.automaticHide && !barHover.hovered)revealHide.restart();}}
    PanelWindow {
        id:revealEdge;screen:bar.screen;visible:bar.automaticHide && !bar.chromeShown
        anchors {top:true;left:true;right:true}implicitHeight:1;exclusiveZone:0;color:"transparent"
        WlrLayershell.namespace:"lumen-menu-reveal";WlrLayershell.layer:WlrLayer.Overlay;WlrLayershell.keyboardFocus:WlrKeyboardFocus.None
        MouseArea {anchors.fill:parent;hoverEnabled:true;acceptedButtons:Qt.NoButton;onEntered:{bar.edgeRevealed=true;revealHide.restart();}}
    }
    required property var shell
    required property date clockDate
    // Label and headings must use the same committed focus/menu snapshot.
    property bool usesBridge: Quickshell.env("SWAYSOCK") !== ""
    readonly property bool ownedFilesContext:!!shell.ownedContext && !!shell.ownedContext.keyWindow && !!shell.ownedContext.mainWindow && shell.ownedContext.applicationId==="lumen-files"
    property string displayName: ownedFilesContext ? "Files" : usesBridge ? shell.nameForAppId(exported.state.contextAppName || exported.state.appName,exported.state.title) : shell.activeName
    property bool filesFocused:ownedFilesContext || (usesBridge ? exported.state.appName==="org.quickshell" && exported.state.title.startsWith("Files") : !!shell.active && shell.active.title.startsWith("Files"))
    property bool desktopFocused: usesBridge ? exported.state.windowId === 0 : !shell.active
    property bool special: ["Wi-Fi","Bluetooth","VPN","Sound","Controls","Focus","Date","Calendar","Now Playing","Files","Settings","Launcher","Capture"].includes(menu)
    FontMetrics {id:headingMetrics;font.family:Theme.font;font.pixelSize:13}
    readonly property var menuHeadings: {
        if(exported.state.supported && exported.state.menu)return exported.state.menu.children.filter(n=>n.visible).map(n=>({label:exported.label(n.label),exportId:n.id,enabled:n.enabled}));
        return (desktopFocused || filesFocused?["File","Edit","View","Go","Window","Help"]:["Window"]).map(name=>({label:name,heading:name,enabled:true}));
    }
    readonly property var hiddenStatus:statusIcons.layout.overflow.map(id=>({label:statusIcons.labels[id],action:id==="search"?"apps":"status:"+(id==="sound"?"Sound":"Controls")}))
    SettingsState {id:barSettings}
    readonly property real menuBudget:Math.max(0,(captureIndicator.visible?captureIndicator.x:statusIcons.x)-28)
    readonly property int visibleHeadingCount:MenuLayout.fit(menuHeadings.map(n=>headingMetrics.advanceWidth(n.label)+20),menuBudget-brandButton.width-applicationButton.width,38,hiddenStatus.length>0)
    readonly property var overflowItems:menuHeadings.slice(visibleHeadingCount).map(n=>({label:n.label,exportId:n.exportId,action:n.enabled?(n.heading?"heading:"+n.heading:"export-menu"):undefined})).concat(hiddenStatus)
    function menuLayoutStatus(){return {width:width,menuEnd:headings.x+headings.width,statusStart:captureIndicator.visible?captureIndicator.x:statusIcons.x,clockWidth:clockButton.width,visible:visibleHeadingCount,headings:menuHeadings.length,overflow:overflowItems.length,overflowLabels:overflowItems.map(n=>n.label)};}
    property int builtinIndex:-1
    property var builtinSelection:null
    property bool builtinResetting:false
    property int builtinSession:0
    property var menuWindow:null
    property var menuFilesWindow:null
    property int menuFocusEpoch:0
    property var menuExportState:null
    function launcherStatus() {return launcherPanel.status();}
    function builtinEnabled(row){return !!row && !row.separator && row.enabled!==false && !!(row.action || row.win);}
    function sameBuiltin(a,b){return !!a && !!b && a.action===b.action && a.win===b.win && a.exportId===b.exportId && a.label===b.label;}
    function windowGroup(target){
        if(!target)return {windows:[],records:[]};
        if(target.appId!=="org.quickshell")return {windows:shell.windows.filter(w=>w.appId===target.appId).slice(),records:[]};
        const records=shell.ownedToplevelAssociations || [];
        const owner=records.find(item=>item.toplevel===target && item.window && item.window.visible && item.main && item.main.visible);
        if(!owner)return {windows:[],records:[]};
        const members=records.filter(item=>item.applicationId===owner.applicationId && shell.windows.includes(item.toplevel) && item.window && item.window.visible && item.main && item.main.visible);
        return {windows:members.map(item=>item.toplevel),records:members};
    }
    function captureBuiltin(row){
        if(!menu || special || !builtinEnabled(row))return null;
        const target=row.win || menuWindow,group=windowGroup(target);
        return {session:builtinSession,menu:menu,row:row,target:target,appId:target?target.appId:"",windows:group.windows,owners:group.records.map(record=>({record:record,applicationId:record.applicationId,toplevel:record.toplevel,window:record.window,main:record.main})),files:menuFilesWindow,epoch:menuFocusEpoch,exportState:menuExportState};
    }
    function validBuiltin(intent){
        if(!menu || special || !intent || intent.session!==builtinSession || intent.menu!==menu || !builtinItems.model.some(row=>builtinEnabled(row) && sameBuiltin(row,intent.row)))return false;
        const a=intent.row.action;
        if(intent.row.win || ["focus","fullscreen","minimize","fill","close","close-all"].includes(a)){
            if(!intent.target || !shell.windows.includes(intent.target) || intent.target.appId!==intent.appId)return false;
            if(a==="close-all" && intent.windows.some(w=>w && shell.windows.includes(w) && w.appId!==intent.appId))return false;
            if(a==="close-all" && intent.appId==="org.quickshell")return intent.owners.length>0 && intent.owners.every(owner=>(shell.ownedToplevelAssociations || []).includes(owner.record) && owner.record.applicationId===owner.applicationId && owner.record.toplevel===owner.toplevel && owner.record.window===owner.window && owner.record.main===owner.main && owner.window && owner.window.visible && owner.main && owner.main.visible && shell.windows.includes(owner.toplevel));
            return true;
        }
        if(a && a.startsWith("files:"))return !!intent.files && shell.filesWindow===intent.files && shell.focusEpoch===intent.epoch;
        if(a==="export-menu")return exported.state===intent.exportState;
        return true;
    }
    function selectBuiltin(index){
        const row=builtinItems.model[index];builtinIndex=index;builtinSelection=captureBuiltin(row);
        const item=builtinItems.itemAt(index);if(item && item.enabled){item.forceActiveFocus();const y=item.y;builtinScroll.contentY=Math.max(0,Math.min(builtinScroll.contentHeight-builtinScroll.height,y-builtinScroll.height/2));}
    }
    function stepBuiltin(delta) {
        const rows=builtinItems.model;
        for(let n=1;n<=rows.length;n++) {
            const start=builtinIndex<0?(delta>0?-1:0):builtinIndex;
            const i=(start+n*delta+rows.length*2)%rows.length;
            if(builtinEnabled(rows[i])){selectBuiltin(i);return;}
        }
    }
    function edgeBuiltin(last){builtinIndex=-1;stepBuiltin(last?-1:1);}
    function activateSelection(){const intent=builtinSelection;builtinSelection=null;if(intent)activateBuiltin(intent.row,intent);}
    function activateBuiltin(modelData,captured) {
        const intent=captured || captureBuiltin(modelData);if(!validBuiltin(intent))return;
        const a=intent.row.action;modelData=intent.row;const target=intent.target;
        if(a==="export-menu"){const context=bar.headingContext();bar.menu="";if(!bar.validHeadingContext(context))return;exported.openMenu(modelData.exportId,Math.min(bar.menuX,bar.width-220),bar.screen,context);return;}
        if(a && a.startsWith("heading:")){bar.menu=a.substring(8);return;}
        if(a && a.startsWith("status:")){bar.menu=a.substring(7);return;}
        if(a && a.startsWith("files:"))shell.filesAction(a.substring(6));
        else if (modelData.win) shell.focusWindow(target);
        else if(a==="focus") shell.focusWindow(target);
        else if(["fullscreen","minimize","fill"].includes(a)) shell.windowAction(target,a);
        else if(a==="close") target.close();
        else if(a==="close-all") intent.windows.filter(w=>w && shell.windows.includes(w)).forEach(w=>w.close());
        else if(a==="apps") {}
        else if(a==="terminal") AppLaunch.run(["foot"]);
        else if(a==="sound") AppLaunch.run(["pavucontrol"]);
        else if(a==="lock") AppLaunch.run(["lumen-lock"]);
        else if(a==="help") shell.openHelp();
        else if(a==="show-desktop") shell.revealDesktop();
        else if(a && a.startsWith("session:")) shell.sessionAction(a.substring(8));
        else if(a==="reference") AppLaunch.run(["xdg-open","http://127.0.0.1:8768/"]);
        else if(a==="mute") shell.audioKey(a);
        else if(a==="up" || a==="down") shell.audioKey(a);
        if(a==="apps") bar.menu="Launcher";
        else if(a==="settings" || a==="about"){bar.menu="";shell.openSettings(a==="about"?"General":"");}
        else if(a || modelData.win) bar.menu="";
    }
    property string systemMenu:""
    Timer {id:menuFade;interval:Theme.preferences.reduceMotion?0:150;onTriggered:bar.systemMenu=""}
    property string menu: ""
    Component.onCompleted:{outputName=screen.name;shell.bars[outputName]=bar}
    Component.onDestruction:delete shell.bars[bar.outputName]
    function exportX(id) {for(let i=0;i<exportedHeadings.count;i++){const b=exportedHeadings.itemAt(i);if(b.modelData.exportId===id)return b.mapToItem(bar.contentItem,0,0).x;}return overflowButton.mapToItem(bar.contentItem,0,0).x;}
    function headingRing(){return [{heading:"Desktop",x:brandButton.mapToItem(bar.contentItem,0,0).x},{heading:"Application",x:applicationButton.mapToItem(bar.contentItem,0,0).x}].concat(menuHeadings.filter(n=>n.enabled).map(n=>Object.assign({},n,{x:n.exportId!==undefined?exportX(n.exportId):headingX(n.heading)})));}
    function headingX(name){for(let i=0;i<exportedHeadings.count;i++){const b=exportedHeadings.itemAt(i);if(b.modelData.heading===name)return b.mapToItem(bar.contentItem,0,0).x;}return overflowButton.mapToItem(bar.contentItem,0,0).x;}
    function headingContext(){return {window:menuWindow,hasWindow:!!menuWindow,files:menuFilesWindow,epoch:menuFocusEpoch,appName:menuAppName,exportState:menuExportState};}
    function validHeadingContext(context){return !(!context || (context.hasWindow && (!context.window || !shell.windows.includes(context.window))) || (context.exportState && context.exportState.supported && (context.exportState.token!==exported.latestState.token || context.exportState.revision!==exported.latestState.revision)));}
    function switchHeading(delta,exportId){
        const ring=headingRing(),i=ring.findIndex(n=>exportId!==undefined?n.exportId===exportId:n.heading===menu);
        if(i<0 || !ring.length)return;
        const context=exportId!==undefined?exported.invokingContext:headingContext();
        if(!validHeadingContext(context)){menu="";exported.path=[];return;}
        const target=ring[(i+delta+ring.length)%ring.length];
        if(target.exportId!==undefined){menu="";exported.openMenu(target.exportId,target.x,screen,context);Qt.callLater(()=>{if(exported.path[0]===target.exportId)exported.step(1);});return;}
        exported.path=[];shell.claimPopup("bar:"+outputName);
        menuWindow=context.window;selectedWindow=context.window;menuFilesWindow=context.files;menuFocusEpoch=context.epoch;menuAppName=context.appName;menuExportState=context.exportState;
        menuX=target.x;menu=target.heading;Qt.callLater(()=>bar.stepBuiltin(1));
    }
    function popupLayout() {
        const p=special?(menu==="Launcher"?launcherPanel:["Files","Settings"].includes(menu)?previewPanel:systemViewport):builtinPanel;
        return {screen:bar.outputName,menu:menu,selectedIndex:builtinIndex,width:dismiss.width,height:dismiss.height,x:p.x+p.width*(1-p.scale)/2*(p===previewPanel?1:0),y:p.y+p.height*(1-p.scale)/2*(p===previewPanel?1:0),panelWidth:p.width*p.scale,panelHeight:p.height*p.scale,contentHeight:systemPanel.height,scrollY:systemViewport.contentY,scale:systemViewport.scale,calendar:menu==="Date"?systemPanel.dateStatus():null};
    }
    property real menuX: 12
    property var selectedWindow: null
    property string menuAppName: ""
    function toggle(name, x) {
        if(name==="Settings"){shell.openSettings("");return;}if(name==="Files"){shell.openFiles("");return;}
        const closing=menu===name;
        const opening=menu==="";const target=opening?shell.active:menuWindow;
        if(menu === "") menuAppName=bar.displayName;
        shell.claimPopup("bar:"+bar.outputName);
        if(opening){menuWindow=target;selectedWindow=target;menuFilesWindow=shell.filesWindow || null;menuExportState=exported.state;}
        menuFocusEpoch=shell.focusEpoch || 0;
        menuX=x; menu=closing?"":name;
    }
    onMenuChanged: {builtinSession++;builtinSelection=null;if(["Wi-Fi","Bluetooth","VPN","Sound","Controls","Focus","Date","Calendar","Now Playing"].includes(menu)){menuFade.stop();systemMenu=menu;systemViewport.contentY=0;}else menuFade.restart();builtinIndex=-1;Qt.callLater(()=>{if(bar && popupContents && bar.menu && bar.menu!=="Launcher")popupContents.forceActiveFocus();});if(menu==="" && shell.popupOwner==="bar:"+bar.outputName)shell.popupOwner="";}
    Connections {target:shell; function onDismissPopups(owner) {if(owner!=="bar:"+bar.outputName)bar.menu="";} }
    Connections { target: shell; function onRequestMenu(name, outputName) { if(outputName!==bar.outputName)return; bar.toggle(name,["Date","Calendar","Sound","Controls"].includes(name)?bar.width-380:16); } }
    anchors { top: true; left: true; right: true }
    implicitHeight: Theme.barHeight
    exclusiveZone:bar.outputFullscreen || bar.automaticHide?0:Theme.barHeight
    WlrLayershell.layer:bar.outputFullscreen || bar.automaticHide?WlrLayer.Overlay:WlrLayer.Top
    WlrLayershell.keyboardFocus:WlrKeyboardFocus.None
    color: Theme.compositorEffects && !Theme.preferences.reduceTransparency ? (Theme.dark ? "#d028282b" : "#d8f2f2f4") : Theme.colors.background
    WlrLayershell.namespace: "lumen-menu"
    Rectangle { anchors.fill: parent; color: "transparent" }
    Row {
        id:headings;x:16;anchors.verticalCenter:parent.verticalCenter;spacing:0
        MenuButton {id:brandButton;text:"";iconName:"lumen";strong:true;selected:bar.menu==="Desktop";onClicked:bar.toggle("Desktop",mapToItem(bar.contentItem,0,0).x)}
        MenuButton {id:applicationButton;width:Math.min(implicitWidth,Math.max(60,Math.min(160,bar.menuBudget*0.4)));text:bar.menu!==""?bar.menuAppName:bar.displayName;strong:true;selected:bar.menu==="Application";onClicked:{bar.selectedWindow=shell.active;bar.toggle("Application",mapToItem(bar.contentItem,0,0).x);}}
        Repeater {
            id:exportedHeadings;model:bar.menuHeadings.slice(0,bar.visibleHeadingCount)
            MenuButton {required property var modelData;text:modelData.label;enabled:modelData.enabled;opacity:enabled?1:0.45
                selected:modelData.heading?bar.menu===modelData.heading:exported.path.length>0 && exported.path[0]===modelData.exportId && exported.output===bar.screen
                onEntered:if(modelData.exportId!==undefined && exported.path.length && exported.path[0]!==modelData.exportId)exported.openMenu(modelData.exportId,mapToItem(bar.contentItem,0,0).x,bar.screen)
                onClicked:{bar.selectedWindow=shell.active;if(modelData.heading)bar.toggle(modelData.heading,mapToItem(bar.contentItem,0,0).x);else{const context=bar.menu?bar.headingContext():undefined;bar.menu="";if(context && !bar.validHeadingContext(context))return;exported.openMenu(modelData.exportId,mapToItem(bar.contentItem,0,0).x,bar.screen,context);}}
            }
        }
        MenuButton {id:overflowButton;visible:bar.overflowItems.length>0;text:"»";selected:bar.menu==="Overflow";Accessible.name:"More menus and status controls";onClicked:{bar.selectedWindow=shell.active;bar.toggle("Overflow",mapToItem(bar.contentItem,0,0).x);}}
    }
    CaptureIndicator {
        id:captureIndicator;state:bar.shell.capture || null
        anchors.right:statusIcons.left;anchors.verticalCenter:parent.verticalCenter
        width:28;selected:bar.menu==="Capture"
        onClicked:bar.toggle("Capture",bar.width-370)
    }
    StatusStrip {
        id:statusIcons
        anchors.right:parent.right;anchors.rightMargin:clockButton.width+12;anchors.verticalCenter:parent.verticalCenter
        order:Theme.menuBarPreferences.order
        budget:Math.max(38,Math.min(222,bar.width-clockButton.width-220-(captureIndicator.visible?28:0)))
        power:shell.systemState && shell.systemState.power ? shell.systemState.power : null
        soundIcon:shell.volume.includes("MUTED")?"muted":"sound"
        available:({battery:!!(shell.systemState && shell.systemState.power && shell.systemState.power.available && shell.systemState.power.present),sound:true,focus:!!shell.notifications.dnd,search:true,controls:true})
        onActivated:id=>{if(id==="search")shell.launchApps();else bar.toggle(id==="sound"?"Sound":"Controls",bar.width-370);}
        onReordered:order=>barSettings.setMenuBarPreference("order",order)
    }
    MenuButton {
        anchors.right: parent.right; anchors.rightMargin: 12; anchors.verticalCenter: parent.verticalCenter
        id:clockButton;width:Math.min(implicitWidth,Math.max(80,bar.width*0.35)); textSize: 13; selected:bar.menu==="Date"
        text:ClockFormat.text(bar.clockDate,Qt.locale(Theme.menuBarPreferences.locale || ""),Theme.menuBarPreferences)
        Accessible.name:text+", open notifications and calendar"
        onClicked: bar.toggle("Date", bar.width-370)
    }
    // A separate layer surface allows Escape/outside-click dismissal without
    // stealing keyboard focus during normal bar use.
    PanelWindow {
        id: dismiss
        screen: bar.screen
        visible: bar.menu!=="" || menuFade.running || (shell.popupOwner!=="" && !(shell.popupOwner==="exported" && exported.output===bar.screen) && shell.popupOwner!=="dock:"+bar.outputName)
        anchors { top:true; bottom:true; left:true; right:true }
        exclusionMode: ExclusionMode.Ignore
        mask:Region {y:bar.special?0:Theme.barHeight;width:dismiss.width;height:dismiss.height-y}
        color: "transparent"
        WlrLayershell.namespace: "lumen-menu-popup"
        WlrLayershell.layer: WlrLayer.Overlay
        WlrLayershell.keyboardFocus: bar.menu!==""?WlrKeyboardFocus.Exclusive:WlrKeyboardFocus.None
        MouseArea { anchors.fill: parent; onClicked: shell.claimPopup("") }
        Item { id:popupContents;anchors.fill:parent;focus:true
            Keys.onEscapePressed:bar.menu=""
            Keys.onDownPressed:event=>{if(!bar.special){bar.stepBuiltin(1);event.accepted=true;}else event.accepted=false;}
            Keys.onUpPressed:event=>{if(!bar.special){bar.stepBuiltin(-1);event.accepted=true;}else event.accepted=false;}
            Keys.onRightPressed:event=>{if(!bar.special){bar.switchHeading(1);event.accepted=true;}else event.accepted=false;}
            Keys.onLeftPressed:event=>{if(!bar.special){bar.switchHeading(-1);event.accepted=true;}else event.accepted=false;}
            Keys.onReturnPressed:event=>{if(!bar.special){bar.activateSelection();event.accepted=true;}else event.accepted=false;}
            Keys.onSpacePressed:event=>{if(!bar.special){bar.activateSelection();event.accepted=true;}else event.accepted=false;}
            Keys.onTabPressed:event=>{if(!bar.special){bar.stepBuiltin(1);event.accepted=true;}else event.accepted=false;}
            Keys.onBacktabPressed:event=>{if(!bar.special){bar.stepBuiltin(-1);event.accepted=true;}else event.accepted=false;}
            Keys.onPressed:event=>{if(!bar.special && (event.key===Qt.Key_Home || event.key===Qt.Key_End)){bar.edgeBuiltin(event.key===Qt.Key_End);event.accepted=true;}else event.accepted=false;}
        FocusFlickable {
            id:systemViewport
            visible:["Wi-Fi","Bluetooth","VPN","Sound","Controls","Focus","Date","Calendar","Now Playing","Capture"].includes(bar.systemMenu)
            width:systemPanel.width;height:Math.min(systemPanel.height,Math.max(1,parent.height-Theme.barHeight-24))
            x:Math.max(8,parent.width-width-12);y:Theme.barHeight+10
            contentWidth:width;contentHeight:systemPanel.height;clip:true;boundsBehavior:Flickable.StopAtBounds
            ScrollBar.vertical:DesktopScrollBar {}
            opacity:bar.menu!=="" && bar.menu===bar.systemMenu?1:0
            transform:Translate {x:bar.systemMenu==="Date"?(1-systemViewport.opacity)*20:0}
            Behavior on opacity {NumberAnimation {duration:Theme.preferences.reduceMotion?0:140}}
            SystemPanels {
                id:systemPanel;maximumWidth:Math.max(1,dismiss.width-24)
                kind:bar.systemMenu;shell:bar.shell;today:bar.clockDate
                onSettingsRequested:bar.shell.openSettings(bar.menu==="Sound"?"Sound":bar.menu==="Date"?"Notifications":"")
                onPanelRequested:name=>bar.menu=name
            }
        }

        AppLauncher {
            id:launcherPanel
            visible:bar.menu==="Launcher"
            x:(parent.width-width)/2;y:Math.max(Theme.barHeight+8,Math.min(parent.height*0.16,parent.height-height-92))
            shell:bar.shell
            onCloseRequested:bar.menu=""
        }
        AppPreview {
            id:previewPanel
            visible:bar.menu==="Files"
            anchors.centerIn:parent;anchors.verticalCenterOffset:(Theme.barHeight-92)/2
            kind:visible?bar.menu:""
            onCloseRequested:bar.menu=""
        }
        PanelSurface {
            id:builtinPanel
            Accessible.role:Accessible.PopupMenu
            Accessible.name:bar.menu
            visible:bar.menu!=="" && !bar.special
            x: Math.max(8,Math.min(bar.menuX,parent.width-width-8)); y: Theme.barHeight+1; width: Math.max(220,Math.min(360,builtinItems.model.reduce((w,n)=>Math.max(w,plainMetrics.advanceWidth(n.label || "")+54),0))); height: Math.min(items.height+12,parent.height-Theme.barHeight-100)
            radius: Theme.menuRadius; color: Theme.colors.background; border.color: Theme.outline; border.width: 1
            FontMetrics {id:plainMetrics;font.family:Theme.font;font.pixelSize:13}
            MouseArea { anchors.fill: parent }
            Flickable {
                id:builtinScroll;anchors.fill:parent;anchors.margins:6;contentWidth:width;contentHeight:items.height;clip:true;boundsBehavior:Flickable.StopAtBounds
            Column {
                id: items; width:parent.width; spacing:1
                Repeater {
                    id:builtinItems
                    onModelChanged:{bar.builtinResetting=true;bar.builtinIndex=-1;bar.builtinSelection=null;Qt.callLater(()=>{if(!bar || !popupContents)return;if(bar.menu && !bar.special)popupContents.forceActiveFocus();bar.builtinResetting=false;});}
                    model: {
                        if(bar.menu==="Overflow")return bar.overflowItems;
                        if(bar.filesFocused){const rows=shell.filesItems(bar.menu);if(rows)return rows;}
                        if (bar.menu.startsWith("Unassigned: ")) return [{label:bar.menu.substring(12) + " is unassigned"},{label:"Open Applications…",action:"apps"}];
                        if (bar.menu === "File") return [{label:"Open Application…",action:"apps"},{label:"New Terminal",action:"terminal"}].concat(bar.menuWindow ? [{label:"Close Window",action:"close"}] : []);
                        // These commands remain disabled until app-exported menus are integrated.
                        if (bar.menu === "Edit") return [{label:"Undo"},{label:"Redo"},{label:"Cut"},{label:"Copy"},{label:"Paste"},{label:"Select All"}];
                        if (bar.menu === "View") return bar.menuWindow ? [{label:bar.menuWindow.fullscreen ? "Exit Full Screen" : "Enter Full Screen",action:"fullscreen"}] : [{label:"Enter Full Screen"}];
                        if (bar.menu === "Go") return [{label:"Applications…",action:"apps"},{label:"Terminal",action:"terminal"}];
                        if (bar.menu === "Window") return (bar.menuWindow ? [{label:"Minimize",action:"minimize"},{label:"Fill / Restore",action:"fill"},{label:bar.menuWindow.fullscreen?"Exit Full Screen":"Enter Full Screen",action:"fullscreen"},{label:"",separator:true}] : []).concat(shell.windows.map(w=>({label:w.title || w.appId,win:w})));
                        if (bar.menu === "Application") return bar.menuWindow ? [{label:"Bring to Front",action:"focus"},{label:"Close Window",action:"close"},{label:"Close All Windows",action:"close-all",enabled:bar.windowGroup(bar.menuWindow).windows.length>0}] : [{label:"Show Applications",action:"apps"}];
                        if (bar.menu === "Sound") return [{label:shell.volume || "Audio unavailable"},{label:"Mute / Unmute",action:"mute"},{label:"Volume Up",action:"up"},{label:"Volume Down",action:"down"},{label:"Sound Settings…",action:"sound"}];
                        if (bar.menu === "Date") return [{label:Qt.formatDateTime(bar.clockDate,"dddd, d MMMM yyyy")},{label:Qt.formatDateTime(bar.clockDate,"h:mm AP")}];
                        if (bar.menu === "Help") return [{label:"Search Desktop Help…",action:"help"},{label:"Reference Board…",action:"reference"}];
                        return [{label:"About This Computer",action:"about"},{label:"System Settings…",action:"settings"},{label:"",separator:true},{label:"Applications…",action:"apps"},...(shell.nativeShowDesktop?[{label:"Show Desktop",action:"show-desktop"}]:[]),{label:"",separator:true},{label:"Sleep…",action:"session:suspend"},{label:"Restart…",action:"session:reboot"},{label:"Shut Down…",action:"session:poweroff"},{label:"",separator:true},{label:"Lock Screen",action:"lock"},{label:"Log Out…",action:"session:logout"}];
                    }
                    AbstractButton {
                        id:builtinRow
                        required property var modelData
                        required property int index
                        property var initialRow:null
                        property var pressedIntent:null
                        Component.onCompleted:initialRow=modelData
                        objectName:"builtin-menu-row-"+index
                        enabled:bar.builtinEnabled(modelData)
                        activeFocusOnTab:enabled
                        Accessible.role:modelData.separator?Accessible.Separator:Accessible.MenuItem
                        Accessible.name:modelData.label || ""
                        Accessible.selected:bar.builtinIndex===index
                        Accessible.onPressAction:trigger()
                        function trigger(){const intent=pressedIntent || bar.captureBuiltin(initialRow);pressedIntent=null;if(intent)bar.activateBuiltin(intent.row,intent);}
                        onPressed:pressedIntent=bar.captureBuiltin(initialRow)
                        onCanceled:pressedIntent=null
                        onClicked:trigger()
                        onActiveFocusChanged:if(activeFocus && !bar.builtinResetting)bar.selectBuiltin(index)
                        onHoveredChanged:if(hovered && enabled){bar.builtinIndex=index;bar.builtinSelection=null;}
                        Keys.onPressed:event=>{
                            if(event.key===Qt.Key_Escape){bar.menu="";event.accepted=true;}
                            else if(event.key===Qt.Key_Down || event.key===Qt.Key_Tab){bar.stepBuiltin(event.modifiers & Qt.ShiftModifier?-1:1);event.accepted=true;}
                            else if(event.key===Qt.Key_Up || event.key===Qt.Key_Backtab){bar.stepBuiltin(-1);event.accepted=true;}
                            else if(event.key===Qt.Key_Left || event.key===Qt.Key_Right){bar.switchHeading(event.key===Qt.Key_Right?1:-1);event.accepted=true;}
                            else if(event.key===Qt.Key_Home || event.key===Qt.Key_End){bar.edgeBuiltin(event.key===Qt.Key_End);event.accepted=true;}
                            else if(event.key===Qt.Key_Return || event.key===Qt.Key_Enter){if(!event.isAutoRepeat)bar.activateSelection();event.accepted=true;}
                            else event.accepted=false;
                        }
                        width:items.width;height:modelData.separator?9:Theme.menuRowHeight
                        background:Rectangle {radius:5;color:bar.builtinIndex===builtinRow.index && builtinRow.enabled?Theme.colors.accent:"transparent"}
                        contentItem:Item {
                            SystemGlyph {visible:!builtinRow.modelData.separator && name!=="";x:7;anchors.verticalCenter:parent.verticalCenter;width:14;height:14;name:(builtinRow.modelData.label || "").startsWith("About")?"info":builtinRow.modelData.action==="settings"?"gear":builtinRow.modelData.action==="apps"?"grid":builtinRow.modelData.action==="terminal"?"screen":builtinRow.modelData.action==="lock"?"lock":"";ink:bar.builtinIndex===builtinRow.index && builtinRow.enabled?Theme.colors.selectedText:Theme.colors.text}
                            Text {anchors.left:parent.left;anchors.leftMargin:28;anchors.right:parent.right;anchors.rightMargin:8;anchors.verticalCenter:parent.verticalCenter;visible:!builtinRow.modelData.separator;text:builtinRow.modelData.label || "";elide:Text.ElideRight;color:builtinRow.enabled?(bar.builtinIndex===builtinRow.index?Theme.colors.selectedText:Theme.colors.text):Theme.colors.muted;font.family:Theme.font;font.pixelSize:13}
                            Rectangle {visible:!!builtinRow.modelData.separator;anchors.centerIn:parent;width:parent.width-12;height:1;color:Theme.colors.border;opacity:0.5}
                        }
                    }
                }
            }
        }
    }
}
}
}

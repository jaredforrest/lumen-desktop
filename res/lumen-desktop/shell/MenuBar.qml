import QtQuick
import QtQuick.Controls
import Quickshell
import Quickshell.Wayland
import "CalendarLogic.js" as CalendarLogic

PanelWindow {
    id: bar
    property var exported: shell.exportedMenu
    property string outputName:""
    required property var shell
    required property date clockDate
    // Label and headings must use the same committed focus/menu snapshot.
    property bool usesBridge: Quickshell.env("SWAYSOCK") !== ""
    property string displayName: usesBridge ? shell.nameForAppId(exported.state.appName,exported.state.title) : shell.activeName
    property bool filesFocused:usesBridge ? exported.state.appName==="org.quickshell" && exported.state.title.startsWith("Files") : !!shell.active && shell.active.title.startsWith("Files")
    property bool desktopFocused: usesBridge ? exported.state.windowId === 0 : !shell.active
    property bool special: ["Wi-Fi","Bluetooth","Sound","Controls","Date","Calendar","Now Playing","Files","Settings","Launcher"].includes(menu)
    property int builtinIndex:-1
    function launcherStatus() {return launcherPanel.status();}
    function stepBuiltin(delta) {
        const rows=builtinItems.model;
        for(let n=1;n<=rows.length;n++) {
            const start=builtinIndex<0?(delta>0?-1:0):builtinIndex;
            const i=(start+n*delta+rows.length*2)%rows.length,r=rows[i];
            if(r.action || r.win){builtinIndex=i;const y=i*(Theme.menuRowHeight+1);builtinScroll.contentY=Math.max(0,Math.min(builtinScroll.contentHeight-builtinScroll.height,y-builtinScroll.height/2));return;}
        }
    }
    function activateBuiltin(modelData) {
        const a=modelData.action;
        if(a && a.startsWith("files:"))shell.filesAction(a.substring(7));
        else if (modelData.win) shell.focusWindow(modelData.win);
        else if(a==="focus" && bar.selectedWindow) shell.focusWindow(bar.selectedWindow);
        else if(["fullscreen","minimize","fill"].includes(a) && bar.selectedWindow) shell.windowAction(bar.selectedWindow,a);
        else if(a==="close" && bar.selectedWindow) bar.selectedWindow.close();
        else if(a==="close-all" && bar.selectedWindow) shell.windows.filter(w=>w.appId===bar.selectedWindow.appId).forEach(w=>w.close());
        else if(a==="apps") {}
        else if(a==="terminal") AppLaunch.run(["foot"]);
        else if(a==="sound") AppLaunch.run(["pavucontrol"]);
        else if(a==="lock") AppLaunch.run(["lumen-lock"]);
        else if(a==="help") shell.openHelp();
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
    function exportX(id) {for(let i=0;i<exportedHeadings.count;i++){const b=exportedHeadings.itemAt(i);if(b.modelData.id===id)return b.mapToItem(bar.contentItem,0,0).x;}return 16;}
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
        if(menu === "") menuAppName=bar.displayName;
        shell.claimPopup("bar:"+bar.outputName);
        menuX=x; menu=closing?"":name;
    }
    onMenuChanged: {if(["Wi-Fi","Bluetooth","Sound","Controls","Date","Calendar","Now Playing"].includes(menu)){menuFade.stop();systemMenu=menu;systemViewport.contentY=0;}else menuFade.restart();builtinIndex=-1;Qt.callLater(()=>{if(bar && popupContents && bar.menu && bar.menu!=="Launcher")popupContents.forceActiveFocus();});if(menu==="" && shell.popupOwner==="bar:"+bar.outputName)shell.popupOwner="";}
    Connections {target:shell; function onDismissPopups(owner) {if(owner!=="bar:"+bar.outputName)bar.menu="";} }
    Connections { target: shell; function onRequestMenu(name, outputName) { if(outputName!==bar.outputName)return; bar.toggle(name,["Date","Calendar","Sound","Controls"].includes(name)?bar.width-380:16); } }
    anchors { top: true; left: true; right: true }
    implicitHeight: Theme.barHeight
    exclusiveZone: Theme.barHeight
    color: Theme.compositorEffects && !Theme.preferences.reduceTransparency ? (Theme.dark ? "#d028282b" : "#d8f2f2f4") : Theme.colors.background
    WlrLayershell.namespace: "lumen-menu"
    Rectangle { anchors.fill: parent; color: "transparent" }
    Row {
        x: 16; anchors.verticalCenter: parent.verticalCenter; spacing: 0
        MenuButton { text: ""; iconName: "lumen"; strong: true; selected:bar.menu==="Desktop"; onClicked: bar.toggle("Desktop", parent.x+x) }
        MenuButton { text: bar.menu !== "" ? bar.menuAppName : bar.displayName; strong: true; selected:bar.menu==="Application"; onClicked: {bar.selectedWindow=shell.active;bar.toggle("Application", parent.x+x);} }
        Repeater {
            id:exportedHeadings
            model: exported.state.supported && exported.state.menu ? exported.state.menu.children.filter(n=>n.visible) : []
            MenuButton {
                required property var modelData
                text: exported.label(modelData.label)
                enabled: modelData.enabled
                selected:exported.path.length>0 && exported.path[0]===modelData.id && exported.output===bar.screen
                onEntered:if(exported.path.length && exported.path[0]!==modelData.id)exported.openMenu(modelData.id,mapToItem(bar.contentItem,0,0).x,bar.screen)
                onClicked: { bar.menu=""; exported.openMenu(modelData.id, mapToItem(bar.contentItem,0,0).x,bar.screen); }
            }
        }
        MenuButton { visible: !exported.state.supported && (bar.desktopFocused || bar.filesFocused); text: "File"; selected:bar.menu==="File"; onClicked: {bar.selectedWindow=shell.active;bar.toggle("File", parent.x+x);} }
        MenuButton { visible: !exported.state.supported && (bar.desktopFocused || bar.filesFocused); text: "Edit"; selected:bar.menu==="Edit"; onClicked: bar.toggle("Edit", parent.x+x) }
        MenuButton { visible: !exported.state.supported && (bar.desktopFocused || bar.filesFocused); text: "View"; selected:bar.menu==="View"; onClicked: {bar.selectedWindow=shell.active;bar.toggle("View", parent.x+x);} }
        MenuButton { visible: !exported.state.supported && (bar.desktopFocused || bar.filesFocused); text: "Go"; selected:bar.menu==="Go"; onClicked: bar.toggle("Go", parent.x+x) }
        MenuButton { visible: !exported.state.supported; text: "Window"; selected:bar.menu==="Window"; onClicked: bar.toggle("Window", parent.x+x) }
        MenuButton { visible: !exported.state.supported && (bar.desktopFocused || bar.filesFocused); text: "Help"; selected:bar.menu==="Help"; onClicked: bar.toggle("Help", parent.x+x) }
    }
    Row {
        anchors.right: parent.right; anchors.rightMargin: clockButton.width+12; anchors.verticalCenter: parent.verticalCenter; spacing: 0
        MenuButton { text: ""; iconName: shell.volume.includes("MUTED") ? "muted" : "sound"; onClicked: bar.toggle("Sound", bar.width-370) }
        MenuButton {visible:shell.notifications.dnd;text:"";iconName:"moon";onClicked:bar.toggle("Controls",bar.width-340)}
        MenuButton { text: ""; iconName: "search"; onClicked: shell.launchApps() }
        MenuButton { text: ""; iconName: "controls"; onClicked: bar.toggle("Controls", bar.width-340) }

    }
    MenuButton {
        anchors.right: parent.right; anchors.rightMargin: 12; anchors.verticalCenter: parent.verticalCenter
        id:clockButton; textSize: 13; selected:bar.menu==="Date"
        text: Qt.formatDateTime(bar.clockDate,CalendarLogic.clockDatePattern(Qt.locale(),Locale.LongFormat))+"  "+Qt.formatTime(bar.clockDate,Qt.locale().timeFormat(Locale.ShortFormat))
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
            Keys.onDownPressed:if(!bar.special)bar.stepBuiltin(1)
            Keys.onUpPressed:if(!bar.special)bar.stepBuiltin(-1)
            Keys.onReturnPressed:if(!bar.special && bar.builtinIndex>=0)bar.activateBuiltin(builtinItems.model[bar.builtinIndex])
        Flickable {
            id:systemViewport
            visible:["Wi-Fi","Bluetooth","Sound","Controls","Date","Calendar","Now Playing"].includes(bar.systemMenu)
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
                    model: {
                        if(bar.filesFocused){const rows=shell.filesItems(bar.menu);if(rows)return rows;}
                        if (bar.menu.startsWith("Unassigned: ")) return [{label:bar.menu.substring(12) + " is unassigned"},{label:"Open Applications…",action:"apps"}];
                        if (bar.menu === "File") return [{label:"Open Application…",action:"apps"},{label:"New Terminal",action:"terminal"}].concat(bar.selectedWindow ? [{label:"Close Window",action:"close"}] : []);
                        // These commands remain disabled until app-exported menus are integrated.
                        if (bar.menu === "Edit") return [{label:"Undo"},{label:"Redo"},{label:"Cut"},{label:"Copy"},{label:"Paste"},{label:"Select All"}];
                        if (bar.menu === "View") return bar.selectedWindow ? [{label:bar.selectedWindow.fullscreen ? "Exit Full Screen" : "Enter Full Screen",action:"fullscreen"}] : [{label:"Enter Full Screen"}];
                        if (bar.menu === "Go") return [{label:"Applications…",action:"apps"},{label:"Terminal",action:"terminal"}];
                        if (bar.menu === "Window") return (bar.selectedWindow ? [{label:"Minimize",action:"minimize"},{label:"Fill / Restore",action:"fill"},{label:bar.selectedWindow.fullscreen?"Exit Full Screen":"Enter Full Screen",action:"fullscreen"},{label:"",separator:true}] : []).concat(shell.windows.map(w=>({label:w.title || w.appId,win:w})));
                        if (bar.menu === "Application") return bar.selectedWindow ? [{label:"Bring to Front",action:"focus"},{label:"Close Window",action:"close"},{label:"Close All Windows",action:"close-all"}] : [{label:"Show Applications",action:"apps"}];
                        if (bar.menu === "Sound") return [{label:shell.volume || "Audio unavailable"},{label:"Mute / Unmute",action:"mute"},{label:"Volume Up",action:"up"},{label:"Volume Down",action:"down"},{label:"Sound Settings…",action:"sound"}];
                        if (bar.menu === "Date") return [{label:Qt.formatDateTime(bar.clockDate,"dddd, d MMMM yyyy")},{label:Qt.formatDateTime(bar.clockDate,"h:mm AP")}];
                        if (bar.menu === "Help") return [{label:"Search Desktop Help…",action:"help"},{label:"Reference Board…",action:"reference"}];
                        return [{label:"About This Computer",action:"about"},{label:"System Settings…",action:"settings"},{label:"",separator:true},{label:"Applications…",action:"apps"},{label:"",separator:true},{label:"Sleep…",action:"session:suspend"},{label:"Restart…",action:"session:reboot"},{label:"Shut Down…",action:"session:poweroff"},{label:"",separator:true},{label:"Lock Screen",action:"lock"},{label:"Log Out…",action:"session:logout"}];
                    }
                    Rectangle {
                        required property var modelData
                        required property int index
                        width:items.width; height:modelData.separator?9:Theme.menuRowHeight; radius:5
                        color: (bar.builtinIndex===index) && (modelData.action || modelData.win) ? Theme.colors.accent : "transparent"
                        SystemGlyph {visible:!modelData.separator && name!=="";x:7;anchors.verticalCenter:parent.verticalCenter;width:14;height:14;name:(modelData.label || "").startsWith("About")?"info":modelData.action==="settings"?"gear":modelData.action==="apps"?"grid":modelData.action==="terminal"?"screen":modelData.action==="lock"?"lock":"";ink:bar.builtinIndex===index && modelData.action?Theme.colors.selectedText:Theme.colors.text}
                        Text { anchors.left:parent.left; anchors.leftMargin:28; anchors.right:parent.right; anchors.rightMargin:8; anchors.verticalCenter:parent.verticalCenter; visible:!modelData.separator; text: modelData.label; elide:Text.ElideRight; color:(modelData.action || modelData.win) ? (bar.builtinIndex===index ? Theme.colors.selectedText : Theme.colors.text) : Theme.colors.muted; font.family:Theme.font; font.pixelSize:13 }
                        Rectangle {visible:!!modelData.separator;anchors.centerIn:parent;width:parent.width-12;height:1;color:Theme.colors.border;opacity:0.5}
                        MouseArea {
                            id:rowMouse; anchors.fill:parent; hoverEnabled:true
                            onEntered:bar.builtinIndex=index
                            onClicked: {
                                bar.activateBuiltin(modelData);
                            }
                        }
                    }
                }
            }
        }
    }
}
}
}

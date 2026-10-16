import QtQuick
import Quickshell
import Quickshell.Io
import Quickshell.Wayland
import "ReferenceDock.js" as ReferenceDock
import "DockModel.js" as DockModel
import "DockInteraction.js" as DockInteraction
import "AppSwitcherModel.js" as SwitcherModel

ShellRoot {
    id: root
    signal requestMenu(string name, string outputName)
    signal requestDockFocus(string outputName)
    signal dismissPopups(string owner)
    property var bars: ({})
    property string popupOwner: ""
    property alias exportedMenu: exported
    GlobalMenu { id:exported; shell:root; holdDisplay:root.popupOwner!=="" && root.popupOwner!=="exported" }
    property int focusEpoch:0
    function claimPopup(owner) { ++focusEpoch;popupOwner=owner; dismissPopups(owner); }
    Connections {target:Quickshell; function onScreensChanged() {
        const names=Quickshell.screens.map(s=>s.name);
        if((root.popupOwner.startsWith("bar:") || root.popupOwner.startsWith("dock:")) && !names.includes(root.popupOwner.substring(root.popupOwner.indexOf(":")+1)))root.claimPopup("");
        if(exported.path.length && (!exported.output || !names.includes(exported.output.name)))root.claimPopup("");
    }}
    function preferredOutput() { return exported.state.outputName || (Quickshell.screens.length?Quickshell.screens[0].name:""); }
    function showMenu(name, outputName) { if(name==="Settings"){openSettings("");return;}if(name==="Files"){openFiles("");return;} requestMenu(name,outputName || preferredOutput()); }
    RemoteDesktopWindow {id:remoteDesktopWindow;shell:root}
    function openRemoteDesktop(){remoteDesktopWindow.show();focusOwnedWindow("Remote Desktop");}
    SessionOptions {id:sessionOptions}
    HelpWindow {id:helpWindow;shell:root}
    function sessionAction(action){claimPopup("");sessionOptions.show(action);focusOwnedWindow("Session Options");}
    function openHelp(){claimPopup("");helpWindow.show();focusOwnedWindow("Desktop Help");}
    FolderStack {id:downloadsStack;shell:root}
    QuickLookWindow {id:quickLook}
    property alias quickLookWindow:quickLook
    TextToolsWindow {id:textTools;shell:root}
    property alias textToolsWindow:textTools
    AppSwitcher {id:applicationSwitcher;shell:root}
    WindowOverview {id:overview;shell:root}
    property alias overviewWindow:overview
    function openTextTools(){textTools.show();focusOwnedWindow("Text Tools");}
    function appSwitcherCommand(action){applicationSwitcher.command(action);}
    function openOverview(sameApp){overview.show(sameApp && active?(active.appId==="org.quickshell"?(active.title.startsWith("Files")?"Files":active.title):active.appId):"");if(overview.visible)focusOwnedWindow("Window Overview");}
    CalendarWindow {id:calendarWindow;shell:root}
    function openCalendar(){calendarWindow.open();}
    function openCalendarAt(day,event){calendarWindow.openAt(day,event);}
    function openSettings(page) { claimPopup("");settingsWindow.openPage(page);focusOwnedWindow("System Settings"); }
    SettingsState {id:settingsState;active:settingsWindow.visible}
    SettingsWindow {id:settingsWindow;shell:root;service:settingsState}
    function openFiles(path) {claimPopup("");filesWindow.openPath(path);}
    property var filesWindow:primaryFiles
    readonly property var filesState:filesWindow.service
    property var filesInstances:[]
    FilesState {id:primaryFilesState}
    FilesWindow {id:primaryFiles;shell:root;service:primaryFilesState}
    Component {id:filesFactory
        QtObject {property FilesState state:FilesState {}
            property FilesWindow window:FilesWindow {shell:root;service:parentState.state}
            id:parentState
        }
    }
    function newFilesWindow(path){
        let instance=filesInstances.find(i=>!i.window.visible && !i.window.minimized);
        if(!instance){instance=filesFactory.createObject(root);instance.window.windowNumber=filesInstances.length+2;filesInstances=filesInstances.concat([instance]);}
        filesWindow=instance.window;filesWindow.openPath(path || filesState.path);
    }
    function activeFiles(window){filesWindow=window;}

    function filesItems(menu) {
        const selected=!!filesWindow.current,ready=!filesState.busy;
        const item=(label,action,enabled=true)=>({label:label,action:enabled && ready?"files:"+action:""});
        if(menu==="File")return [item("Open","open",selected),item("Get Info","info",selected),item("New Window","new-window"),item("New Tab","new-tab"),item("New Folder…","mkdir",filesState.writable),item("Rename…","rename",selected),item("Move to Trash…","trash",selected)];
        if(menu==="Edit")return [item("Cut","cut",selected),item("Copy","copy",selected),item("Paste","paste",filesState.writable),item("Undo Last Move","undo",filesState.canUndo)];
        if(menu==="View")return [item("as Icons","grid"),item("as List","list"),item(filesWindow.previewOpen?"Hide Preview":"Show Preview","preview"),item(filesState.hidden?"Hide Hidden Files":"Show Hidden Files","hidden"),item("Refresh","refresh")];
        if(menu==="Go")return [item("Back","back",filesState.historyIndex>0),item("Forward","forward",filesState.historyIndex<filesState.history.length-1),item("Enclosing Folder","up"),item("Home","home"),item("Go to Folder…","goto")];
        return null;
    }
    function filesAction(action){claimPopup("");filesActionTimer.action=action;filesActionTimer.restart();}
    Timer {id:filesActionTimer;property string action;interval:100;onTriggered:filesWindow.dispatch(action)}
    property var pins: ["brave-browser", "codex-desktop", "obsidian", "bitwarden", "foot", "writer", "calc", "impress", "org.gnome.Evince", "lumen-remote-desktop", "org.inkscape.Inkscape", "org.darktable.darktable", "audacity", "mpv", "com.github.xournalpp.xournalpp", "org.pulseaudio.pavucontrol"]
    property var windowCreationOrder:[]
    property var windows: ToplevelManager.toplevels.values
    onWindowsChanged:windowCreationOrder=SwitcherModel.creationOrder(windowCreationOrder,windows)
    function windowIdentity(window){const entry=entryFor(window);return entry?entry.id:window.appId;}
    function cycleAppWindow(direction){
        const selected=SwitcherModel.cycleWindow(windowCreationOrder,windows,active,windowIdentity,direction==="previous"?-1:1);
        if(selected)focusWindow(selected);
    }
    property var active: ToplevelManager.activeToplevel
    property var activationHistory: []
    onActiveChanged: {
        if (active) activationHistory=[active].concat(activationHistory.filter(w=>w!==active && windows.includes(w)));
    }
    property var entries: DesktopEntries.applications.values
    property var pinned: { const available = entries; return pins.map(id => available.find(e => e.id === id || e.id === id + ".desktop")).filter(e => e !== undefined); }
    property var runningExtras: {
        let result = [];
        for (const w of windows) {
            const e = entryFor(w);
            if (e && !pins.includes(e.id) && !result.some(x => x.id === e.id)) result.push(e);
        }
        return result;
    }
    property var dockApps: pinned.concat(runningExtras)
    property var referenceDock: DockModel.build(ReferenceDock.items, entries, dockApps)
    function dockWindows(item) { if(item.action==="calendar")return windows.filter(w=>w.appId==="org.quickshell" && w.title==="Calendar");if(item.action==="files")return windows.filter(w=>w.appId==="org.quickshell" && w.title.startsWith("Files"));if(item.action==="settings")return windows.filter(w=>w.appId==="org.quickshell" && w.title==="System Settings");return item.entry ? windowsFor(item.entry) : []; }
    function dockLabel(item) { return item.entry ? appLabel(item.entry) : item.name; }
    function activateDock(item, newWindow, outputName, modifiers) {
        if (!item) return;
        if (item.entry && modifiers && (modifiers & Qt.AltModifier) && !(modifiers & Qt.GroupSwitchModifier)) {
            const matches=windowsFor(item.entry);
            const hideOthers=!!(modifiers & Qt.MetaModifier);
            if (matches.length && (hideOthers || matches.includes(active)) && Quickshell.env("SWAYSOCK")) {
                const command=["lumen-focus-window",matches[0].appId,"","--activate-titles",JSON.stringify(matches.map(w=>w.title)),"--app-action",hideOthers?"hide-others":"hide"];
                if (Theme.preferences.switchToAppSpace===false) command.push("--no-switch-space");
                const previous=activationHistory.find(w=>!matches.includes(w) && windows.includes(w));
                if (!hideOthers && previous) command.push("--previous-app",previous.appId,"--previous-title",previous.title);
                Quickshell.execDetached(command);
                return;
            }
        }
        if (item.entry) activate(item.entry, newWindow);
        else if (item.key === "downloads") downloadsStack.show(Quickshell.env("HOME")+"/Downloads",outputName || preferredOutput());
        else if (item.key === "trash") openFiles("trash:");
        else if (item.action === "launcher") showMenu("Launcher",outputName);
        else if (item.action === "calendar") openCalendar();
        else if (["files","settings"].includes(item.action)) showMenu(item.action === "files" ? "Files" : "Settings",outputName);

    }
    property var shortNames: ({"lumen-character-viewer":"Emoji & Symbols", "codex-desktop":"Codex", "brave-browser":"Brave", "foot":"Terminal", "writer":"Writer", "calc":"Calc", "impress":"Impress", "gimp":"GIMP", "org.gnome.Evince":"Preview"})
    property string activeName: active ? (entryFor(active) ? (shortNames[entryFor(active).id] || entryFor(active).name) : nameForAppId(active.appId,active.title)) : "Desktop"
    function nameForAppId(appId,title) {
        if(["org.remmina.Remmina","remmina"].includes(appId))return "Remote Desktop";
        if(appId==="org.quickshell" && title.startsWith("Files"))return "Files";
        if(appId==="org.quickshell")return ["Files","System Settings","Calendar","Text Tools","Window Overview","Quick Look"].includes(title)?title:"Desktop";
        if (!appId || appId === "Desktop") return "Desktop";
        const entry = entryFor({appId:appId});
        return entry ? (shortNames[entry.id] || entry.name) : appId;
    }
    function dockIcon(entry) { return entry.id==="lumen-remote-desktop" ? Qt.resolvedUrl("dock-icons/lumen-remote-desktop.svg") : Quickshell.iconPath(entry.icon,"application-x-executable"); }
    property bool hudVisible: false
    property string hudOutput: ""
    Timer {id:hudTimer;interval:1400;onTriggered:root.hudVisible=false}
    function audioKey(action) {
        if(action!=="up" && action!=="down" && action!=="mute")return;
        if(action==="mute")audio.toggleMute();
        else {audio.setVolume(audio.level+(action==="up"?0.05:-0.05));}
        hudOutput=preferredOutput();hudVisible=true;hudTimer.restart();
    }
    property alias audioState: audio
    readonly property string volume: audio.volume
    function setVolume(value) { audio.setVolume(value); }
    function editVolume(editing) { audio.editing = editing; }
    AudioState { id: audio }
    property alias systemState: controls
    SystemState {id:controls}
    property alias search: searchState
    SearchState {id:searchState;active:Object.values(root.bars).some(b=>b && b.menu==="Launcher")}
    property alias notifications:notificationState
    NotificationState {id:notificationState;shell:root}
    function entryFor(w) {
        if (!w) return null;
        if((w.appId==="org.quickshell" && w.title==="Remote Desktop") || ["org.remmina.Remmina","remmina"].includes(w.appId))return DesktopEntries.byId("lumen-remote-desktop");
        if(w.appId==="org.quickshell" && w.title.startsWith("Files"))return DesktopEntries.byId("lumen-files");
        if(w.appId==="org.quickshell" && w.title==="Calendar")return DesktopEntries.byId("lumen-calendar-app");
        if(w.appId==="org.quickshell" && w.title==="System Settings")return DesktopEntries.byId("lumen-system-settings");
        // StartupWMClass is often different from the desktop filename.
        const match = entries.find(e => e.startupClass && e.startupClass.toLowerCase() === w.appId.toLowerCase());
        return match || DesktopEntries.heuristicLookup(w.appId);
    }
    function windowsFor(entry) { return windows.filter(w => { const e = entryFor(w); return e && e.id === entry.id; }); }
    function focusOwnedWindow(title){ownedFocus.title=title;ownedFocus.epoch=focusEpoch;ownedFocus.restart();}
    Timer {id:ownedFocus;property string title;property int epoch;interval:100;onTriggered:{if(epoch!==root.focusEpoch)return;const window=root.windows.find(w=>w.appId==="org.quickshell" && w.title===title);if(window)root.focusWindow(window);}}
    function windowAction(window, action) {
        if (!window) return;
        if (action==="close") {window.close();return;}
        if (Theme.nativeWindowActions) Quickshell.execDetached(["lumen-focus-window",window.appId,window.title,"--action",action]);
        else if(action==="fullscreen") window.fullscreen=!window.fullscreen;
        else if(action==="minimize") window.minimized=true;
        else if(action==="fill") window.maximized=!window.maximized;
    }
    function focusWindow(window) {
        if (!window) return;
        window.activate();
        focusFallback.targetWindow=window;focusFallback.epoch=focusEpoch;focusFallback.restart();
    }
    Timer {id:focusFallback;property var targetWindow;property int epoch;interval:160;onTriggered:if(epoch===root.focusEpoch && targetWindow && !targetWindow.activated && Quickshell.env("SWAYSOCK"))Quickshell.execDetached(["lumen-focus-window",targetWindow.appId,targetWindow.title])}
    function activate(entry, newWindow) {
        const matches = windowsFor(entry);
        if (newWindow || !matches.length) {
            if(entry.id==="lumen-files"){if(newWindow)newFilesWindow("");else openFiles("");return;}
            if(entry.id==="lumen-system-settings"){openSettings("");return;}
            if(entry.id==="lumen-calendar-app"){openCalendar();return;}
        }
        if (!newWindow && matches.length) {
            const ordered=activationHistory.filter(w=>matches.includes(w)).concat(matches.filter(w=>!activationHistory.includes(w)));
            if (Quickshell.env("SWAYSOCK")) {
                const command=["lumen-focus-window",ordered[0].appId,"","--activate-titles",JSON.stringify(ordered.map(w=>w.title))];
                if (Theme.preferences.switchToAppSpace===false) command.push("--no-switch-space");
                Quickshell.execDetached(command);
            } else focusWindow(DockInteraction.activationWindow(matches,active,activationHistory));
        } else AppLaunch.runEntry(entry, newWindow);
    }
    function appLabel(entry) { return shortNames[entry.id] || entry.name; }
    function appIcon(entry) {
        const item = referenceDock.find(i => i.entry && i.entry.id === entry.id);
        return item ? Qt.resolvedUrl("reference-icons/"+item.image) : (Quickshell.iconPath(entry.icon,true) || Qt.resolvedUrl("icons/application.svg"));
    }
    function launchApps() { showMenu("Launcher"); }
    readonly property var currentDate: clock.date
    SystemClock { id: clock; precision: SystemClock.Minutes }
    IpcHandler {
        function remoteDesktop():void {root.openRemoteDesktop();}
        function calendar():void {root.openCalendar();}
        target: "desktop"
        function status(): string { return JSON.stringify({screens: Quickshell.screens.map(s=>({name:s.name,width:s.width,height:s.height})), active:root.activeName, referenceDock:root.referenceDock.map(e=>({key:e.key,center:e.center,assigned:!!e.entry || !!e.action})), apps:root.dockApps.map(e=>({id:e.id,name:e.name,icon:e.icon,windows:root.windowsFor(e).length})), windows:root.windows.map(w=>({appId:w.appId,title:w.title,active:w.activated})), volume:root.volume}); }
        function activate(id: string): string { const e=DesktopEntries.byId(id); if (!e) return "missing"; root.activate(e,false); return "requested"; }
        function menu(name: string): void { root.showMenu(name); }
        function layout():string {return JSON.stringify(Object.values(root.bars).filter(b=>b && typeof b.popupLayout==="function").map(b=>b.popupLayout()));}
        function menuOn(name:string, outputName:string):void {root.showMenu(name,outputName);}
        function audioKey(action:string):void {root.audioKey(action);}
        function controlsStatus():string {return JSON.stringify({audio:{available:audio.available,level:audio.level,muted:audio.muted,outputs:audio.outputs.map(n=>n.description)},wifiAvailable:controls.wifi.available,focus:{available:notificationState.available,enabled:notificationState.dnd}});}
        function notificationCentre(action:string):void {
            if(!["open","close","toggle"].includes(action))return;
            const name=root.preferredOutput(),bar=root.bars[name];if(!bar)return;
            if(action==="close" || (action==="toggle" && bar.menu==="Date")){if(bar.menu==="Date")bar.menu="";}
            else if(bar.menu!=="Date")root.showMenu("Date",name);
        }
        function notificationStatus():string {return JSON.stringify({available:notificationState.available,dnd:notificationState.dnd,count:notificationState.history.length,banners:notificationState.banners.length,unread:notificationState.unread,output:notificationState.bannerOutput});}
        function toggleFocus():void {notificationState.command({action:"dnd"});}
        function searchStatus():string {return JSON.stringify({available:searchState.available,indexed:searchState.indexed,scanning:searchState.scanning,searching:searchState.searching,error:searchState.error,views:Object.values(root.bars).filter(b=>b && b.menu==="Launcher").map(b=>b.launcherStatus())});}
        function files(path:string):void {root.openFiles(path);}
        function filesNew(path:string):void {root.newFilesWindow(path);}
        function textToolsStatus():string {return JSON.stringify({visible:root.textToolsWindow.visible,error:root.textToolsWindow.error,page:root.textToolsWindow.page,snippets:root.textToolsWindow.snippets.length});}
        function textTools():void {root.openTextTools();}
        function recognizeImage(path:string):void {root.claimPopup("");quickLook.show(path,null);quickLook.recognizeOnLoad=true;root.focusOwnedWindow("Quick Look");}
        function folderStackStatus():string {return JSON.stringify({visible:downloadsStack.visible,count:downloadsStack.rows.length,error:downloadsStack.error});}
        function focusDock():void {root.requestDockFocus(root.preferredOutput());}
        function cycleWindow(direction:string):void {if(["next","previous"].includes(direction))root.cycleAppWindow(direction);}
        function appSwitcher(action:string):string {root.appSwitcherCommand(action);return applicationSwitcher.status();}
        function overview(sameApp:bool):void {root.openOverview(sameApp);}
        function quickLookStatus():string {return JSON.stringify({visible:root.quickLookWindow.visible,type:root.quickLookWindow.result.type || "",playing:root.quickLookWindow.preview.playing,position:root.quickLookWindow.preview.position,playbackError:root.quickLookWindow.preview.playbackError,page:root.quickLookWindow.result.page || 1,pages:root.quickLookWindow.result.pages || 1,error:root.quickLookWindow.result.error || ""});}
        function overviewStatus():string {return JSON.stringify({visible:root.overviewWindow.visible,count:root.overviewWindow.matches.length,workspaces:root.overviewWindow.workspaces.length,error:root.overviewWindow.error});}
        function filesStatus():string {return JSON.stringify({visible:filesWindow.visible,path:filesState.path,busy:filesState.busy,error:filesState.error,count:filesState.rows.length,selected:filesWindow.current?filesWindow.current.name:"",preview:filesState.preview.type || "",list:filesWindow.listMode,view:filesState.viewMode,columns:filesState.columns.length,selection:filesWindow.selectedPaths,tabs:filesWindow.tabs.length,jobs:filesState.jobs,working:filesState.working,dragging:filesWindow.dragging,windowCount:1+root.filesInstances.length});}
        function settings(page:string):void {root.openSettings(page);}
        function settingsStatus():string {return JSON.stringify({visible:settingsWindow.visible,page:settingsWindow.page,available:settingsState.available,busy:settingsState.busy,error:settingsState.error,preferences:settingsState.preferences});}
        function launcher(): void { root.launchApps(); }
    }
    Variants {
        model: Quickshell.screens
        Scope {
            required property var modelData
            MenuBar { screen: modelData; shell: root; clockDate: clock.date }
            Dock { screen: modelData; shell: root }
            NotificationBanners {screen:modelData;outputName:modelData.name;outputWidth:modelData.width;outputHeight:modelData.height;shell:root}
            VolumeHud {screen:modelData;outputName:modelData.name;shell:root}
        }
    }
}

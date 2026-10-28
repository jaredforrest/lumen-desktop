import QtQuick
import QtQuick.Window
import Quickshell
import Quickshell.Io
import Quickshell.Wayland
import "assets/shell" as D
ShellRoot {id:mock;property int disabledPresses:0
 QtObject {id:service;property bool available:true;property string error:"";property var focusState:({profiles:[{id:"work",name:"Work",allowedApps:[],allowCritical:false,schedules:[]}]});property var applications:[{key:"chat",name:"Chat"}];function command(data){focusState={profiles:[data.profile]};}}
 FloatingWindow {id:window;visible:true;title:"Accessibility Fixture";implicitWidth:600;implicitHeight:850
  Column {width:500;spacing:8;Accessible.role:Accessible.Pane;Accessible.name:"Fixture content"
   D.ControlTile {id:focus;width:300;height:62;label:"Focus";subtitle:active?"Work":"Off";toggle:true;active:false;unavailable:false;onClicked:active=!active}
   D.ControlRow {width:300;text:"For one hour"}
   D.MenuButton {text:"Unavailable command";enabled:false;onClicked:mock.disabledPresses++}
   D.SettingsToggle {width:400;text:"Show Calendar";checked:false}
   D.FocusProfileEditor {service:service;profileId:"work"}
  }
 }
 QtObject {id:widgets;property var desktopItems:[{id:"calendar-one",type:"calendar",size:"small",output:"*",style:"month"},{id:"clock-two",type:"clock",size:"medium",output:"*",hourCycle:"locale"}];function addWidget(type){desktopItems=desktopItems.concat([{id:"new-widget",type:type,size:"small",output:"*",hourCycle:"locale"}]);}function updateWidget(id,changes){desktopItems=desktopItems.map(item=>item.id===id?Object.assign({},item,changes):item);}function removeWidget(id){desktopItems=desktopItems.filter(item=>item.id!==id);}}
 FloatingWindow {id:widgetWindow;visible:false;title:"Widget accessibility";implicitWidth:700;implicitHeight:950;D.FocusFlickable {anchors.fill:parent;anchors.margins:16;contentWidth:width;contentHeight:editor.implicitHeight;D.DesktopWidgetEditor {id:editor;width:parent.width;service:widgets}}}
 QtObject {id:overviewShell;function claimPopup(owner){}}
 D.WindowOverview {id:overview;shell:overviewShell}
 QtObject {id:sessionLifecycle;property bool available:true;property var session:({phase:"idle",token:"",error:""});property var commands:[];signal sessionAuthorized(var authorization);function sessionCommand(action,value){commands=commands.concat([{action:action,value:value}]);if(action==="session_cancel")session={phase:"idle",token:"",error:""};return true;}}
 D.SessionOptions {id:sessionWindow;lifecycle:sessionLifecycle;executeActions:false;onConfirmed:mock.finalActions++}
 property int finalActions:0
 QtObject {id:noticeService;property bool available:true;property var expandedCards:({});property var invocations:[];property var commands:[];function invoke(notification,action){invocations=invocations.concat([{id:notification.id,action:action}]);}function command(value){commands=commands.concat([value]);}function toggleExpanded(id){}}
 QtObject {id:searchService;property var files:[];property var preview:({});property bool searching:false;property bool opening:false;property bool scanning:false;property int indexed:0;property bool limited:false;property bool unreadable:false;property bool more:false;property string error:"";signal opened();function search(value){}function requestPreview(value){}}
 QtObject {id:launchShell;property var search:searchService;property var entries:[];property var notifications:({dnd:false,available:false});property var audioState:({muted:false,available:false});property var exportedMenu:({state:{menu:null},label:function(value){return value;}});property date currentDate:new Date();property var launches:[];function activate(entry,newWindow){launches=launches.concat([entry.id]);}function appLabel(entry){return entry.name;}function appIcon(entry){return "";}}
 FloatingWindow {id:actionsWindow;visible:false;title:"Shell action accessibility";implicitWidth:950;implicitHeight:800
  Column {anchors.fill:parent;spacing:18
   D.AppLauncher {id:launcher;shell:launchShell;onCloseRequested:actionsWindow.visible=false}
   D.NotificationCard {id:notice;width:600;service:noticeService;notification:({id:77,app:"Fixture",summary:"Protocol subject",body:"Protocol body",icon:"",image:"",timestamp:Date.now(),actions:[{id:"default",label:"Open"}]});onExpandRequested:mock.expansions++}
  }
 }
 property int expansions:0
 QtObject {id:dockShell;property var referenceDock:[{key:"fixture-dock",canvas:68,entry:{id:"fixture-dock",name:"Fixture Dock",icon:"application-x-executable"}}];property string popupOwner:"";property bool dockPreferencesBusy:false;property string aliasError:"";property string applicationCommandError:"";property date currentDate:new Date();property var focuses:[];property var activations:[];property var quits:[];property var windows:ToplevelManager.toplevels.values;signal requestDockFocus(string name);signal dismissPopups(string owner);function claimPopup(owner){popupOwner=owner;}function windowsFor(entry){return windows.filter(w=>w.appId==="accessibility-dock-source" || w.appId==="accessibility-dock-other");}function dockWindows(item){return windowsFor(item.entry);}function dockLabel(item){return item.entry.name;}function runningFor(entry){return windowsFor(entry).length>0;}function canConfigureDockEntry(entry){return false;}function isDockPinned(entry){return false;}function canForceQuit(entry){return false;}function focusWindow(window){focuses=focuses.concat([window.title]);window.activate();}function activate(entry,fresh){activations=activations.concat([entry.id]);}function activateDock(item,fresh,output){activations=activations.concat([item.entry.id]);}function updateDockPin(entry,action){}function reorderDockPin(key,index){}function quitApplication(entry){quits=quits.concat([entry.id]);}function forceQuit(entry){}function revealDockAlias(alias){}function removeDockAlias(alias){}}
 D.Dock {id:fixtureDock;screen:Quickshell.screens[0];shell:dockShell}
 IpcHandler {target:"a11yFixture";function status():string {return JSON.stringify({visible:window.visible,focus:focus.active,profile:service.focusState.profiles[0],disabledPresses:mock.disabledPresses});}function close():void {window.visible=false;}
 function dockOpen():void {fixtureDock.openContext(dockShell.referenceDock[0],0);}
 function dockStatus():string {return JSON.stringify({open:fixtureDock.contextEntry!==null,index:fixtureDock.contextIndex,windows:dockShell.windowsFor(dockShell.referenceDock[0].entry).map(w=>w.title),focuses:dockShell.focuses,activations:dockShell.activations,quits:dockShell.quits});}
 function resultsSetup():void {actionsWindow.visible=true;launchShell.entries=[{id:"fixture.alpha",name:"Fixture Alpha",genericName:"Synthetic app",noDisplay:false},{id:"fixture.beta",name:"Fixture Beta",genericName:"Synthetic app",noDisplay:false}];launcher.setProvider("Applications");}
 function resultsGrid():void {launcher.gridMode=true;}
 function resultsRemove():void {launchShell.entries=launchShell.entries.filter(e=>e.id!=="fixture.alpha");}
 function resultsStatus():string {return JSON.stringify({visible:launcher.visible,provider:launcher.provider,query:launcher.queryText,launches:launchShell.launches,results:launcher.results.map(r=>r.id)});}
 function actionsShow():void {actionsWindow.visible=true;}
 function actionsStatus():string {return JSON.stringify({visible:actionsWindow.visible,preview:launcher.previewOpen,grid:launcher.gridMode,invocations:noticeService.invocations,expansions:mock.expansions});}
 function actionsDisableApps():void {D.Theme.searchPreferences={disabledProviders:["applications"]};}
 function actionsEnableApps():void {D.Theme.searchPreferences={disabledProviders:[]};}
 function actionsStack():void {notice.expandStack=true;notice.stackCount=3;}
 function actionsClose():void {actionsWindow.visible=false;}
 function widgetsShow():void {widgetWindow.visible=true;}
 function widgetsClose():void {widgetWindow.visible=false;}
 function widgetsStatus():string {return JSON.stringify(widgets.desktopItems);}
 function groupsShow():void {overview.show("");}
 function groupsClose():void {overview.visible=false;}
 function groupsStatus():string {return JSON.stringify({visible:overview.visible,busy:overview.busy,request:overview.request});}
 function sessionShow():void {sessionWindow.show("reboot");sessionWindow.preflighting=true;sessionLifecycle.session={phase:"waiting",token:"fixture-token",action:"reboot",waiting:"Fixture editor",remaining:1,error:""};}
 function sessionStatus():string {return JSON.stringify({visible:sessionWindow.visible,finalActions:mock.finalActions,commands:sessionLifecycle.commands});}}
}

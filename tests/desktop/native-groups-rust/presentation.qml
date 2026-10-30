import QtQuick
import Quickshell
import Quickshell.Io
import "assets/shell" as D
ShellRoot {
 D.WindowGroupState {id:groups}
 QtObject {id:service;property bool available:true;property bool busy:false;property string error:"";property string warning:"";property var preferences:({mode:"dark"});property var desktopPreferences:({});property var windowsPreferences:D.Theme.windowsPreferences;property var notificationsPreferences:({preview:"always"});property var searchPreferences:({});property var appearance:({});property var system:({});property var menuBarPreferences:({});property var edits:[];function refresh(){} function setPreference(k,v){}function setWindowPreference(k,v){edits=edits.concat([{key:k,value:v}]);D.Theme.windowsPreferences=Object.assign({},D.Theme.windowsPreferences,{[k]:v});}}
 QtObject {id:audio;property bool available:false;property bool muted:false;property real volume:0;property var outputs:[];property var inputs:[];property var streams:[];property bool inputAvailable:false}
 QtObject {id:notifications;property bool available:false;property bool dnd:false;property var applications:[];property var history:[];property var policies:({});property string error:""}
 QtObject {id:searchState;property bool available:false;property bool scanning:false;property int indexed:0}
 QtObject {id:shortcuts;property bool available:false;property string error:"";property var bindings:[]}
 QtObject {id:stub;property var audioState:audio;property var notifications:notifications;property var search:searchState;property var shortcuts:shortcuts;property var screenSaverPreferences:null;property string volume:"";property var windowGroupPresentation:groups.state;property string windowGroupPresentationError:groups.presentationError}
 D.SettingsWindow {id:settings;shell:stub;service:service}
 function find(item,name){if(item.objectName===name)return item;for(const child of item.children || []){const found=find(child,name);if(found)return found;}return null;}
 IpcHandler {target:"groupAudit"
  function status():string{const mode=find(settings.contentItem,"settings-control-group-window-presentation");return JSON.stringify({state:groups.state,desired:groups.desiredMode,error:groups.presentationError,request:groups.modeRequest,worker:groups.modeWorker.running,edits:service.edits,settings:{visible:settings.visible,mode:mode?{enabled:mode.enabled,focus:mode.activeFocus,index:mode.currentIndex}:null}});}
  function desired(value:string):void{D.Theme.windowsPreferences=Object.assign({},D.Theme.windowsPreferences,{groupWindowMode:value});}
  function open():void{settings.openPage("Desktop & Dock");settings.navigate("Desktop & Dock","group-window-presentation");}
  function close():void{settings.visible=false;}
 }
}

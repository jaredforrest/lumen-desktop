import QtQuick
import QtQuick.Window
import Quickshell
import Quickshell.Io
import "assets/shell" as D
ShellRoot {
 QtObject {id:service;property bool available:true;property bool busy:false;property string error:"";property string warning:"";property var preferences:({mode:"dark"});property var desktopPreferences:({});property var windowsPreferences:({});property var notificationsPreferences:({preview:"always"});property var searchPreferences:({});property var appearance:({});property var system:({});property int writes:0;function refresh(){} function setPreference(k,v){writes++;} function setWindowPreference(k,v){writes++;} property var menuBarPreferences:({})}
 QtObject {id:audio;property bool available:false;property bool muted:false;property real volume:0;property var outputs:[];property var inputs:[];property var streams:[];property bool inputAvailable:false}
 QtObject {id:notifications;property bool available:false;property bool dnd:false;property var applications:[];property var history:[];property var policies:({});property string error:""}
 QtObject {id:searchState;property bool available:false;property bool scanning:false;property int indexed:0}
 QtObject {id:shortcuts;property bool available:true;property string error:"";property var bindings:[{enabled:true,action:"overview",chord:"Mod4+Tab",scope:"global",appId:""}]}
 QtObject {id:stub;property var audioState:audio;property var notifications:notifications;property var search:searchState;property var shortcuts:shortcuts;property var screenSaverPreferences:null;property string volume:""}
 D.SettingsWindow {id:editor;shell:stub;service:service}

 function find(name,item){if(item.objectName===name)return item;for(const child of item.children || []){const found=find(name,child);if(found)return found;}return null;}
 function geometry(item){if(!item)return null;const p=item.mapToItem(editor.contentItem,0,0);return {x:p.x,y:p.y,w:item.width,h:item.height,focus:item.activeFocus,hovered:item.hovered===true,enabled:item.enabled};}
 IpcHandler {target:"settingsAudit"
  function state():string {
   const viewport=find("settings-content-scroll",editor.contentItem),search=find("settings-search",editor.contentItem);
   return JSON.stringify({writes:service.writes,visible:editor.visible,active:editor.contentItem.Window.window?editor.contentItem.Window.window.active:false,page:editor.page,control:editor.controlId,search:search.text,searchIndex:editor.searchIndex,results:editor.searchResults,selected:geometry(editor.revealedControl),searchGeometry:geometry(search),windowWidth:editor.width,windowHeight:editor.height,historyIndex:editor.historyIndex,scroll:viewport?{top:viewport.contentY,height:viewport.height}:null,zoomChoices:find("settings-control-title-bar-double-click-action",editor.contentItem).model});
  }
  function result(control:string):string{return JSON.stringify(geometry(find("settings-result-"+control,editor.contentItem)));}
  function reopen():void {editor.openPage();}
 }
 Component.onCompleted:editor.openPage("Appearance")
}

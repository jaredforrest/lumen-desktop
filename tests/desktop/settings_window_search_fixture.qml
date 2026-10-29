import QtQuick
import Quickshell
import "LUMEN_ASSETS" as D
ShellRoot {
 QtObject {id:service;property bool available:true;property bool busy:false;property string error:"";property string warning:"";property var preferences:({mode:"dark"});property var desktopPreferences:({});property var windowsPreferences:({});property var notificationsPreferences:({preview:"always"});property var searchPreferences:({});property var appearance:({});property var system:({});property int writes:0;function refresh(){} function setPreference(k,v){writes++;} function setWindowPreference(k,v){writes++;} property var menuBarPreferences:({})}
 QtObject {id:audio;property bool available:false;property bool muted:false;property real volume:0;property var outputs:[];property var inputs:[];property var streams:[];property bool inputAvailable:false}
 QtObject {id:notifications;property bool available:false;property bool dnd:false;property var applications:[];property var history:[];property var policies:({});property string error:""}
 QtObject {id:searchState;property bool available:false;property bool scanning:false;property int indexed:0}
 QtObject {id:shortcuts;property bool available:true;property string error:"";property var bindings:[{enabled:true,action:"overview",chord:"Mod4+Tab",scope:"global",appId:""}]}
 QtObject {id:stub;property var audioState:audio;property var notifications:notifications;property var search:searchState;property var shortcuts:shortcuts;property var screenSaverPreferences:null;property string volume:""}
 D.SettingsWindow {id:editor;shell:stub;service:service}
 property int step:0
 function find(name,item){if(item.objectName===name)return item;for(const child of item.children || []){const found=find(name,child);if(found)return found;}return null;}
 function check(ok,message){if(!ok)throw Error(message);}
 Timer {interval:100;running:true;repeat:true;onTriggered:{try {
  const search=find("settings-search",editor.contentItem);
  if(step===0){editor.openPage("Appearance");step++;return;}
  if(step===1){check(search!==null,"missing search");search.text="Title bar double-click action";check(editor.searchResults[0].control==="title-bar-double-click-action","wrong exact result");search.accepted();step++;return;}
  if(step===2){check(editor.page==="Desktop & Dock" && editor.controlId==="title-bar-double-click-action","wrong destination");check(editor.revealedControl && editor.revealedControl.activeFocus,"missing exact control focus");const viewport=find("settings-content-scroll",editor.contentItem);const at=editor.revealedControl.mapToItem(viewport.contentItem,0,0);check(at.y>=viewport.contentY && at.y+editor.revealedControl.height<=viewport.contentY+viewport.height,"not revealed "+JSON.stringify({y:at.y,h:editor.revealedControl.height,top:viewport.contentY,height:viewport.height,content:viewport.contentHeight}));editor.navigate("Accessibility","reduce-motion");step++;return;}
  if(step===3){editor.back(-1);step++;return;}
  if(step===4){check(editor.controlId==="title-bar-double-click-action" && editor.revealedControl.activeFocus,"Back lost control");editor.visible=false;editor.openPage();step++;return;}
  if(step===5){check(editor.controlId==="title-bar-double-click-action" && editor.revealedControl.activeFocus,"Reopen lost control");search.text="Dock size";const captured=editor.searchResults[0];search.text="Reduce motion";editor.acceptSearch(captured,"Dock size");check(editor.controlId==="title-bar-double-click-action","stale query activated");search.text="No matching label";search.accepted();check(editor.controlId==="title-bar-double-click-action","empty match changed destination");search.text="Reduce motion";search.accepted();step++;return;}
  if(step===6){check(editor.controlId==="reduce-motion" && editor.revealedControl.activeFocus,"new search focus");editor.navigate("Accessibility","configured-shortcuts");step++;return;}
  check(editor.revealedControl && editor.revealedControl.objectName==="settings-configured-shortcuts","shortcut section missing");check(service.writes===0,"navigation changed preferences");console.log("SETTINGS_SEARCH_QML_PASS");Qt.quit();
 }catch(error){console.error("SETTINGS_SEARCH_QML_FAIL "+error);Qt.quit();}}}
}
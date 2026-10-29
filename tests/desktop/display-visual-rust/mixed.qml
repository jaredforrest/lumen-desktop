import QtQuick
import Quickshell
import Quickshell.Io
import "assets/shell" as D
ShellRoot {id:fixtureRoot
 function pressedButton(object) {if(!object)return null;if(object.pressed===true)return object;for(const child of object.children || []){const found=pressedButton(child);if(found)return found;}return null;}
 function findSearch(object) {if(!object)return null;if(object.placeholderText==="Search Help")return object;for(const child of object.children || []){const found=findSearch(child);if(found)return found;}return null;}
 QtObject {id:stub;property string popupOwner:"";signal dismissPopups(string owner);function claimPopup(owner){popupOwner=owner;dismissPopups(owner);}function launchApps(){}}
 D.HelpWindow {id:help;shell:stub}
 FloatingWindow {id:probe;title:"Scale specimen";visible:false;implicitWidth:520;implicitHeight:360;color:D.Theme.colors.background
  property int actions:0
  D.AppIcon {id:icon;x:24;y:24;width:48;height:48;iconSource:Qt.resolvedUrl("assets/shell/reference-icons/files.svg")}
  D.PanelLabel {id:label;x:88;y:28;text:"0123456789 12:34 — café Ångström"}
  D.DialogButton {id:action;x:24;y:100;text:"Recorded action";onClicked:probe.actions++}
 }
 IpcHandler {target:"scale"
  function openWindow(kind:string):void {help.visible=kind==="owned";probe.visible=kind==="native";}
  function state():string {const search=fixtureRoot.findSearch(help.contentItem);const position=search?search.mapToItem(help.contentItem,0,0):Qt.point(0,0);const held=fixtureRoot.pressedButton(help.contentItem);return JSON.stringify({held:!!held,heldHovered:held?held.hovered:false,search:search?{x:position.x,y:position.y,width:search.width,height:search.height,focused:search.activeFocus}:null,actions:probe.actions,query:help.query,icon:{x:icon.x,y:icon.y,width:icon.width,height:icon.height},label:{x:label.x,y:label.y,width:label.width,height:label.height},action:{x:action.x,y:action.y,width:action.width,height:action.height},help:{width:help.width,height:help.height}});}
 }
}
import QtQuick
import QtQuick.Window
import Quickshell
import Quickshell.Io
import "assets/shell" as D
ShellRoot {
 QtObject {id:stub;property var groupSurfaces:({});property var groupMembershipDrag:dragController;property string popupOwner:"";property int focusEpoch:0;signal requestGroupSelector(string name);signal dismissPopups(string owner);function claimPopup(owner){focusEpoch++;popupOwner=owner;dismissPopups(owner);}}
 D.WindowGroupState {id:groupState}
 D.GroupMembershipDrag {id:dragController;shell:stub;service:groupState}
 D.WindowOverview {id:overview;shell:stub}
 Variants {model:Quickshell.screens;D.WindowGroupSelector {required property var modelData;screen:modelData;outputName:modelData.name;shell:stub;service:groupState}}
 function find(item,name){if(item.objectName===name)return item;for(const child of item.children || []){const found=find(child,name);if(found)return found;}return null;}
 IpcHandler {target:"groupAudit"
  function status():string{return JSON.stringify({state:groupState.state,visible:overview.visible,busy:overview.busy,revision:overview.windowGroups?overview.windowGroups.revision:-1,error:overview.error,drag:{active:dragController.active,keyboard:dragController.keyboardMode,source:dragController.source,target:dragController.target,destinations:dragController.destinations,selected:dragController.selected,commits:dragController.commits,error:dragController.error},surfaces:Object.values(stub.groupSurfaces).map(s=>({output:s.outputName,revealed:s.revealed,membership:s.membershipMode,rows:s.rows}))});}
  function open(id:string):void{overview.showForWindow(Number(id));}
  function refresh():void{overview.send({action:"status"});}
  function forgedOfferAccepted():bool{return dragController.accepts({getDataAsString:function(){return "forged";}});}
  function geometry(name:string,output:string):string{let panel=null,item=null;if(output){panel=stub.groupSurfaces[output];if(panel)item=find(panel.contentItem,name);}else{panel=overview.contentItem.Window.window;item=find(panel.contentItem,name);}if(!item || !panel)return "null";const point=item.mapToItem(panel.contentItem,0,0);return JSON.stringify({x:(output?panel.screen.x:0)+point.x,y:(output?panel.screen.y:0)+point.y,width:item.width,height:item.height,enabled:item.enabled});}
  function focusControl(name:string):bool{const item=find(overview.contentItem.Window.window.contentItem,name);if(!item)return false;item.forceActiveFocus();return true;}
 }
}

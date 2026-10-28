import QtQuick
import Quickshell
import Quickshell.Io
import "assets/shell" as D
ShellRoot {
 QtObject {id:stub;property var groupSurfaces:({});property string popupOwner:"";property int focusEpoch:0;signal requestGroupSelector(string name);signal dismissPopups(string owner);function claimPopup(owner){focusEpoch++;popupOwner=owner;dismissPopups(owner);}}
 D.WindowGroupState {id:groupState}
 Variants {model:Quickshell.screens;D.WindowGroupSelector {required property var modelData;screen:modelData;outputName:modelData.name;shell:stub;service:groupState}}
 function find(item,name){if(item.objectName===name)return item;for(const child of item.children || []){const found=find(child,name);if(found)return found;}return null;}
 IpcHandler {target:"groupAudit"
  function status():string{return JSON.stringify({state:groupState.state,surfaces:Object.values(stub.groupSurfaces).map(s=>({output:s.outputName,visible:s.visible,revealed:s.revealed,keyboard:s.keyboardMode,rows:s.rows,capture:s.capture,selected:s.selected,commits:s.commits,error:s.error}))});}
  function beginFocused():void{stub.requestGroupSelector(groupState.state.focusedOutput || "");}
  function begin(name:string):void{stub.requestGroupSelector(name);}
  function stopObserver():void{groupState.observer.running=false;}
  function geometry(output:string,name:string):string{const panel=stub.groupSurfaces[output];if(!panel)return "null";const item=find(panel.contentItem,name);if(!item)return "null";const point=item.mapToItem(panel.contentItem,0,0);return JSON.stringify({x:panel.screen.x+point.x,y:panel.screen.y+point.y,width:item.width,height:item.height});}
 }
}

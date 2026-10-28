import QtQuick
import QtQuick.Window
import Quickshell
import Quickshell.Io
import "assets/shell" as D
ShellRoot {
 QtObject {id:stub;function claimPopup(owner){}}
 D.WindowOverview {id:overview;shell:stub}
 function find(item,name){if(item.Accessible && item.Accessible.name===name)return item;for(const child of item.children || []){const found=find(child,name);if(found)return found;}if(item.contentItem && item.contentItem!==item){const found=find(item.contentItem,name);if(found)return found;}return null;}
 IpcHandler {target:"membershipAudit"
  function open(id:string):void{overview.showForWindow(Number(id));}
  function status():string{return JSON.stringify({visible:overview.visible,busy:overview.busy,error:overview.error,current:overview.current,choices:overview.assignmentChoices});}
  function prepare():bool{const box=find(overview.contentItem,"Persistent application desktop assignment");const button=find(overview.contentItem,"Assign Application");if(!box || !button)return false;box.currentIndex=overview.assignmentChoices.findIndex(item=>item.kind==="all");button.forceActiveFocus();return button.activeFocus;}
  function refresh():void{overview.send({action:"status"});}
  function close():void{overview.visible=false;}
 }
}

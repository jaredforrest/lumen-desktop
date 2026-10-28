import QtQuick
import QtQuick.Window
import Quickshell
import Quickshell.Io
import "assets/shell" as D
ShellRoot {
 QtObject {id:stub;function claimPopup(owner){}}
 D.WindowOverview {id:overview;shell:stub}
 function find(item,name){if(item.objectName===name || (item.Accessible && item.Accessible.name===name))return item;for(const child of item.children || []){const found=find(child,name);if(found)return found;}if(item.contentItem && item.contentItem!==item){const found=find(item.contentItem,name);if(found)return found;}return null;}
 IpcHandler {target:"desktopAudit"
  function openOverview(id:string):bool{overview.showForWindow(Number(id));return overview.visible;}
  function refreshOverview():void{overview.send({action:"status"});}
  function status():string{return JSON.stringify({visible:overview.visible,busy:overview.busy,error:overview.error,workspaces:overview.workspaces,revision:overview.revision,editing:find(overview.contentItem,"desktop-order-pointer").parent.editing,groups:overview.windowGroups,managed:overview.managedDesktopId});}
  function geometry(name:string):string{const item=find(overview.contentItem.Window.window.contentItem,name);if(!item)return "null";const at=item.mapToItem(overview.contentItem,0,0);return JSON.stringify({x:at.x,y:at.y,width:item.width,height:item.height,title:overview.title});}
  function selectWindow(id:string):bool{const index=overview.matches.findIndex(w=>w.id===Number(id));if(index<0)return false;overview.selected=index;return true;}
  function focusControl(name:string):bool{const item=find(overview.contentItem.Window.window.contentItem,name);if(!item)return false;item.forceActiveFocus();return true;}
  function click(name:string):bool{const item=find(overview.contentItem.Window.window.contentItem,name);if(!item || !item.enabled)return false;item.clicked();return true;}
 }
}

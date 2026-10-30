import QtQuick
import QtQuick.Window
import Quickshell
import Quickshell.Io
import "assets/shell" as D
ShellRoot {
 property double sourceId:0
 property var observedController:null
 D.WindowGroupState {id:groupState}
 QtObject {id:exported;property var state:({appName:"",title:"",windowId:0})}
 QtObject {id:stub;property var windowGroupPresentation:groupState.state;property var groupMembershipDrag:null;property int focusEpoch:0;property var windows:[];property var exportedMenu:exported;function claimPopup(owner){}function activeFiles(window){}function focusWindow(window){} }
 D.FilesState {id:filesService;persistViewPreferences:false;path:Quickshell.env("HOME")}
 D.FilesWindow {id:files;shell:stub;service:filesService;visible:false}
 D.WindowOverview {id:overview;shell:stub}
 function find(item,name){if(item.objectName===name)return item;for(const child of item.children || []){const found=find(child,name);if(found)return found;}return null;}
 function controller(){const handle=find(overview.contentItem,"desktop-window-drag-"+sourceId);if(handle)observedController=handle.controller;return observedController;}
 function outputControl(item){if(item.currentIndex!==undefined && item.model && typeof item.model==="object" && Array.from(item.model).some(row=>row.name==="HEADLESS-1" && row.logicalId==="connector:HEADLESS-1"))return item;for(const child of item.children || []){const found=outputControl(child);if(found)return found;}return null;}
 IpcHandler {target:"groupAudit"
  function status():string{const drag=controller();return JSON.stringify({state:groupState.state,visible:overview.visible,overviewActive:overview.overviewActive,busy:overview.busy,revision:overview.revision,epoch:overview.sessionEpoch,error:overview.error,snapshot:overview.transferSnapshot,windows:overview.windows,desktops:overview.workspaces,drag:drag?{active:drag.active,busy:drag.busy,commits:drag.commits,error:drag.error,request:drag.request}:null});}
  function open(id:string):void{sourceId=Number(id);overview.showForWindow(sourceId);}
  function showFiles():void{files.visible=true;}
  function legacyMove(id:string):void{const row=overview.windows.find(w=>w.id===sourceId);if(row)overview.send({action:"move",id:row.id,pid:row.pid,logicalId:id,sessionEpoch:overview.sessionEpoch});}
  function refresh():void{overview.send({action:"status"});}
  function cardState(id:string):string{const item=find(overview.contentItem,"desktop-window-card-drag-"+id);return item?JSON.stringify({pressed:item.pressed,captured:!!item.captured,enabled:item.enabled,busy:overview.busy}):"null";}
  function refreshCard(id:string):string{const item=find(overview.contentItem,"desktop-window-card-drag-"+id);if(!item)return "null";const before={pressed:item.pressed,captured:!!item.captured,enabled:item.enabled};overview.send({action:"status"});return JSON.stringify({before:before,after:{enabled:item.enabled,pressed:item.pressed,captured:!!item.captured}});}
  function display(name:string):bool{const control=outputControl(overview.contentItem);if(!control)return false;const index=Array.from(control.model).findIndex(row=>row.name===name);if(index<0)return false;control.currentIndex=index;return true;}
  function source():string{return JSON.stringify(overview.captureDesktopWindow(overview.windows.find(w=>w.id===sourceId)));}
  function reveal():void{const index=overview.matches.findIndex(w=>w.id===sourceId);if(index>=0)overview.step(index-overview.selected);}
  function focusControl(name:string):bool{const item=find(overview.contentItem,name);if(!item)return false;item.forceActiveFocus();return true;}
  function create():void{const output=overview.outputs.find(o=>o.name==="HEADLESS-1");if(output)overview.send({action:"desktop-create",outputId:output.logicalId,revision:overview.revision,sessionEpoch:overview.sessionEpoch});}
  function geometry(name:string):string{const panel=overview.contentItem.Window.window,item=find(panel.contentItem,name);if(!item)return "null";const p=item.mapToItem(panel.contentItem,0,0);return JSON.stringify({x:p.x,y:p.y,width:item.width,height:item.height,enabled:item.enabled});}
 }
}

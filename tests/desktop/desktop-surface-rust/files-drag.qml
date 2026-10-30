import QtQuick
import QtQuick.Window
import Quickshell
import Quickshell.Io
import "@ASSETS@" as D
ShellRoot {
 id:root
 property var nativeDrop:null
 property var dropEvents:[]
 QtObject {id:exported;property var state:({appName:"",title:"",windowId:0})}
 QtObject {id:stub;property int focusEpoch:0;property var windows:[];property var exportedMenu:exported;function claimPopup(name){}function focusOwnedWindow(name){}function focusWindow(window){}function activeFiles(window){}function openSettings(name){}}
 D.FilesState {id:service;persistViewPreferences:false;path:@FOLDER@;viewMode:"list"}
 D.FilesWindow {id:files;shell:stub;service:service;visible:true}
 Component.onCompleted:{service.navigate(@FOLDER@);Qt.callLater(()=>root.nativeDrop=root.walk(files.contentItem).find(item=>item.objectName==="files-native-drop"));}
 Connections {target:root.nativeDrop
  function onEntered(drag){root.dropEvents=root.dropEvents.concat([{kind:"entered",x:drag.x,y:drag.y,targets:root.targets(),hasUrls:drag.hasUrls,supportedActions:drag.supportedActions,proposedAction:drag.proposedAction,accepted:drag.accepted}]).slice(-64);}
  function onDropped(drop){root.dropEvents=root.dropEvents.concat([{kind:"dropped",x:drop.x,y:drop.y,targets:root.targets(),hasUrls:drop.hasUrls,supportedActions:drop.supportedActions,proposedAction:drop.proposedAction,action:drop.action,accepted:drop.accepted}]).slice(-64);}
 }
 function walk(item){let result=[item];for(const child of item.children || [])result=result.concat(walk(child));return result;}
 function visible(item){while(item && item!==files.contentItem){if(!item.visible)return false;item=item.parent;}return item===files.contentItem;}
 function targets(){return root.nativeDrop?root.nativeDrop.targets.filter(item=>!!item).map(item=>{const at=item.mapToItem(files.contentItem,item.width/2,item.height/2);return {folder:item.folder,visible:root.visible(item),ownVisible:item.visible,enabled:item.enabled,x:at.x,y:at.y,width:item.width,height:item.height};}):[];}
 function point(path){const item=walk(files.contentItem).find(item=>item.modelData && item.modelData.path===path && item.width>40 && item.height>15 && visible(item));if(!item)return null;const at=item.mapToItem(files.contentItem,item.width/2,item.height/2);return {x:at.x,y:at.y};}
 IpcHandler {target:"filesDragTest"
  function status():string{return JSON.stringify({ready:service.ready,title:files.title,busy:service.busy,working:service.working,jobs:service.jobs,canUndo:service.canUndo,undoKind:service.undoKind,undoLabel:service.undoLabel,contentFocus:root.walk(files.contentItem).some(item=>item.objectName==="files-keyboard-content" && item.activeFocus),error:service.error,view:service.viewMode,path:service.path,rows:service.rows,selected:files.selectedPaths,dragging:files.dragging,nativeDrop:root.nativeDrop?{containsDrag:root.nativeDrop.containsDrag,destination:root.nativeDrop.destination,x:root.nativeDrop.x,y:root.nativeDrop.y}:null,dropEvents:root.dropEvents,targets:root.targets(),points:service.rows.map(row=>({path:row.path,at:root.point(row.path)})),width:files.width,height:files.height});}
  function clearError():void{service.error="";}
  function refresh():void{service.refresh();}
  function mode(value:string):void{service.viewMode=value;}
  function selectAll():void{files.selectedPaths=service.rows.map(row=>row.path);files.selected=0;}
  function navigate(path:string):void{files.navigate(path);}
  function selectPath(path:string):void{const index=service.rows.findIndex(row=>row.path===path);if(index<0)throw new Error("Exact requested row is absent");files.selected=index;files.selectedPaths=[path];}
  function putBackSelected():void{if(service.path!=="trash:" || files.selectedPaths.length!==1 || !service.rows.some(row=>row.path===files.selectedPaths[0]))throw new Error("Exact selected Trash row required");service.startJob("restore",files.selectedPaths);}
 }
}

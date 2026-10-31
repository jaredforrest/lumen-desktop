import QtQuick
import QtQuick.Window
import Quickshell
import Quickshell.Io
import "assets/shell" as D
ShellRoot {
 id:fixture
 property var observedBand:null
 property string lastCancel:""
 property int beganCount:0
 property var events:[]
 function observe(kind){const b=fixture.band();events=events.concat([{kind:kind,mode:service.viewMode,band:String(b),view:b?String(b.view):"",pending:!!b && b.pending,activeFocus:!!b && b.activeFocus,focusItem:String(files.activeFocusItem),contentY:b?b.view.contentY:null,columns:service.columns.map(c=>({path:c.path,loaded:c.loaded,rows:c.rows.length}))}]).slice(-96);}
 Connections {target:fixture.observedBand;function onBegan(){fixture.beganCount++;fixture.observe("began");}function onCanceled(reason){fixture.lastCancel=reason;fixture.observe("cancel:"+reason);}function onActiveFocusChanged(){fixture.observe("band-focus");}}
 Connections {target:service;function onColumnsChanged(){fixture.observe("columns");}}
 QtObject {id:exported;property var state:({appName:"",title:"",windowId:0})}
 D.ScreenSaverState {id:permission;active:true}
 Window {id:bystander;visible:false;title:"Selection bystander";width:240;height:100;color:"#303030"}
 QtObject {id:stub;property int focusEpoch:0;property var windows:[];property var exportedMenu:exported;property var screenSaverPreferences:permission;function activeFiles(window){}function focusWindow(window){}}
 D.FilesState {id:service;persistViewPreferences:false;preferencesLoaded:true;path:__DOCS__;viewMode:"list"}
 D.FilesWindow {id:files;shell:stub;service:service;visible:true}
 Component.onCompleted:service.navigate(service.path)
 function find(name,item){if(item.objectName===name)return item;for(const child of item.children || []){const found=find(name,child);if(found)return found;}return null;}
 function band(){return find("files-"+(service.viewMode==="columns"?"column-band-"+service.path:service.viewMode+"-band-selection"),files.contentItem);}
 IpcHandler {id:selectionAuditHandler;target:"selectionAudit"
  function mode(value:string):void {service.viewMode=value;fixture.observe("mode");}
  function trace():string {return JSON.stringify(fixture.events);}
  function mark():string {fixture.observe("before-begin");return JSON.stringify(fixture.events);}
  function bandState():string {const b=fixture.band();return JSON.stringify({band:b?String(b):null,view:b?String(b.view):null,pending:!!b && b.pending,active:!!b && b.active,focus:!!b && b.activeFocus,preview:files.bandPreviewPaths,selection:files.selectedPaths,contentX:b?b.view.contentX:null,contentY:b?b.view.contentY:null,columnsReady:service.columns.length>0 && service.columns.every(c=>c.loaded===true),columns:service.columns.map(c=>({path:c.path,loaded:c.loaded,rows:c.rows.length}))});}
  function replayColumnReply(kind:string):string {const columns=service.columns.slice(),index=columns.findIndex(c=>kind==="ancestor"?c.path!==service.path:c.path===service.path);if(index<0)return JSON.stringify({error:"No captured column"});const old=columns[index],before=JSON.parse(selectionAuditHandler.bandState()),receipt={kind:kind,path:old.path,index:index,rowCount:old.rows.length,before:before};columns[index]=Object.assign({},old,{rows:kind==="current"?old.rows.slice().reverse():old.rows,loaded:true});if(kind==="path")columns[index]=Object.assign({},columns[index],{path:old.path+"-retired"});if(kind==="count")columns.splice(index,1);fixture.observe("replay-before:"+kind);service.columns=columns;fixture.observe("replay-after:"+kind);return JSON.stringify(receipt);}
  function reloadColumns():void {service.loadColumns();}
  function showBystander(value:bool):void {bystander.visible=value;}
  function refresh():void {service.refresh();}
  function reverse():void {service.descending=!service.descending;service.refresh();}
  function state():string {const b=fixture.band();return JSON.stringify({ready:service.ready && !service.busy && service.rows.length===160,mode:service.viewMode,selection:files.selectedPaths,rowPaths:service.rows.map(row=>row.path),preview:files.bandPreviewPaths,current:files.current?files.current.path:null,pending:!!b && b.pending,active:!!b && b.active,error:service.error,dragging:files.dragging,lastCancel:fixture.lastCancel,beganCount:fixture.beganCount,bandFocus:!!b && b.activeFocus,lock:permission.lock,permitted:permission.permitted,filesActive:files.selectionActive});}
  function geometry():string {const b=fixture.band();if(!b || !b.permitted)return "null";fixture.observedBand=b;const boxes=b.boxes().filter(box=>box.x>=b.view.contentX && box.y>=b.view.contentY && box.x+box.width<b.view.contentX+b.width && box.y+box.height<b.view.contentY+b.height);if(!boxes.length)return "null";const first=boxes[0],at=b.mapToItem(files.contentItem,0,0),horizontal=service.viewMode==="gallery";const start=horizontal?{x:first.x+first.width/2,y:b.view.contentY+b.height-b.bottomInset-6}:{x:first.x+first.width+6,y:first.y+first.height/2};const end={x:first.x+1,y:first.y+1};return JSON.stringify({debug:{parentIsView:b.parent===b.view,parentType:String(b.parent),viewType:String(b.view),visible:b.visible,enabled:b.enabled,indexAtStart:b.view.indexAt(start.x,start.y),width:b.width,height:b.height,rightInset:b.rightInset,contentX:b.view.contentX,contentY:b.view.contentY,at:at,first:first,areas:b.children.filter(child=>child.acceptedButtons!==undefined).map(child=>({width:child.width,height:child.height,enabled:child.enabled,visible:child.visible}))},step:service.viewMode==="grid"?Math.max(1,Math.floor(b.view.width/b.view.cellWidth)):1,start:{x:at.x+start.x-b.view.contentX,y:at.y+start.y-b.view.contentY},end:{x:at.x+end.x-b.view.contentX,y:at.y+end.y-b.view.contentY},row:{x:at.x+first.x-b.view.contentX+first.width/2,y:at.y+first.y-b.view.contentY+first.height/2},path:first.row.path,populated:horizontal?b.view.contentWidth>b.width:b.view.contentHeight>b.height});}
 }
}

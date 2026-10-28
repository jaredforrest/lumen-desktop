import QtQuick
import QtQuick.Window
import Quickshell
import Quickshell.Io
import "assets/shell" as D
ShellRoot {
  id: fixture
  property var latest: ({})
  QtObject { id: exported; property var state: ({appName:"",title:"",windowId:0}) }
  QtObject { id: stub; property int focusEpoch:0; property var windows:[]; property var exportedMenu:exported; function activeFiles(window){} function focusWindow(window){} }
  D.FilesState { id: service; persistViewPreferences:false; preferencesLoaded:true; path:__DOCS__; viewMode:"list"; onBatchRenameReady:reply=>{if(reply.action!=="rename-cancel")fixture.latest=reply} }
  D.FilesWindow { id: files; shell:stub; service:service; visible:true }
  Component.onCompleted: service.navigate(service.path)
  function tree(){const window=files.contentItem.Window.window;return window?window.contentItem:files.contentItem;}
  function find(name,item){if(item.objectName===name)return item;for(const child of item.children || []){const found=find(name,child);if(found)return found;}return null;}
  IpcHandler { target:"batchAudit"
    function state():string {const confirm=fixture.find("batch-rename-confirm",fixture.tree()),first=fixture.find("batch-rename-first",fixture.tree());return JSON.stringify({ready:service.ready && !service.busy && service.rows.length>0,rows:service.rows,selection:files.selectedPaths,reply:fixture.latest,open:!!first && first.visible,canConfirm:!!confirm && confirm.enabled,error:service.error});}
    function geometry(name:string):string {const item=fixture.find(name,fixture.tree());if(!item)return "null";const at=item.mapToItem(files.contentItem,0,0);return JSON.stringify({x:at.x,y:at.y,width:item.width,height:item.height,visible:item.visible});}
    function refresh():void {service.refresh();}
  }
}

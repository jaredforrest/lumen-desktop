import QtQuick
import Quickshell
import Quickshell.Io
import "assets/shell" as D
ShellRoot {
  id: fixture
  QtObject {id:exported;property var state:({appName:"",title:"",windowId:0})}
  QtObject {id:stub;property int focusEpoch:0;property var windows:[];property var exportedMenu:exported;function activeFiles(window){}function focusWindow(window){}}
  D.FilesState {id:service;persistViewPreferences:false;path:"trash:";viewMode:"list"}
  D.FilesWindow {id:files;shell:stub;service:service;visible:true;implicitWidth:760;implicitHeight:560}
  Component.onCompleted:files.openPath("trash:")
  function find(name,item){if(!item)return null;if(item.objectName===name)return item;for(const child of item.children || []){const result=find(name,child);if(result)return result;}return null;}
  function controller(){return find("trash-actions-state",files.contentItem);}
  function content(){return files.contentItem.Window.window.contentItem;}
  IpcHandler {target:"trashAudit"
    function state():string {const state=fixture.controller(),confirm=fixture.find("trash-confirm",fixture.content());return JSON.stringify({ready:service.ready && !service.busy,rows:service.rows.map(row=>({path:row.path,identity:row.identity})),error:service.error,controller:!!state,busy:state?state.busy:false,waiting:state?state.waiting:false,confirmation:state?state.confirmation:null,operations:state?state.operations:[],reason:state?state.error:"",confirmVisible:confirm?confirm.visible:false,focus:fixture.content().Window.window.activeFocusItem?fixture.content().Window.window.activeFocusItem.objectName:""});}
    function prepare(mode:string,path:string):void {files.selectedPaths=path?[path]:[];files.selected=service.rows.findIndex(row=>row.path===path);files.prepareDeletion(mode);}
    function refresh():void {service.refresh();}
    function closeResults():void {const done=fixture.find("trash-results-done",fixture.content());if(done)done.clicked();}
    function geometry(name:string):string {const item=fixture.find(name,fixture.content());if(!item)return "null";const at=item.mapToItem(files.contentItem,0,0);return JSON.stringify({x:at.x,y:at.y,width:item.width,height:item.height});}
  }
}

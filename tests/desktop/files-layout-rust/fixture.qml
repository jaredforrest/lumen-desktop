import QtQuick
import Quickshell
import Quickshell.Io
import "assets/shell" as D
import "assets/shell/FilesTabModel.js" as Tabs
ShellRoot {
 id:fixture
 property var paths:__PATHS__
 QtObject {id:exported;property var state:({appName:"",title:"",windowId:0})}
 QtObject {id:stub;property int focusEpoch:0;property var windows:[];property var exportedMenu:exported;function activeFiles(window){}function focusWindow(window){}}
 D.FilesState {id:service;persistViewPreferences:false;path:fixture.paths.First;viewMode:"list";favorites:[{name:"First",path:fixture.paths.First},{name:"Second",path:fixture.paths.Second},{name:"Third",path:fixture.paths.Third}]}
 D.FilesWindow {id:files;shell:stub;service:service;visible:true}
 Component.onCompleted:{files.tabs=["First","Second","Third"].map((name,index)=>{const tab=Tabs.create(name.toLowerCase(),paths[name]);tab.history=index===0?[paths.First]:[paths.First,paths[name]];tab.historyIndex=index===0?0:1;tab.selectedPaths=[paths[name]+"/selected.txt"];tab.selectedPath=tab.selectedPaths[0];tab.viewMode="list";return tab;});service.history=files.tabs[0].history.slice();service.historyIndex=0;service.navigate(paths.First,0);}
 function find(name,item){if(item.objectName===name)return item;for(const child of item.children || []){const found=find(name,child);if(found)return found;}return null;}
 IpcHandler {target:"filesAudit"
  function state():string {return JSON.stringify({ready:service.ready && !service.busy && service.rows.length>0,tabs:files.tabs,activeId:files.activeTabId,path:service.path,history:service.history,historyIndex:service.historyIndex,selection:files.selectedPaths,sidebarWidth:files.sidebarWidth,savedWidth:service.sidebarWidth,collapsed:service.collapsedSidebarGroups,width:files.width,resizeFocus:fixture.find("files-sidebar-resize",files.contentItem).activeFocus,resizeFacts:fixture.find("files-sidebar-resize",files.contentItem).children.filter(item=>item.pressed!==undefined).map(item=>({pressed:item.pressed,cancelled:item.gestureCancelled})),focused:files.contentItem.Window.window && files.contentItem.Window.window.activeFocusItem?files.contentItem.Window.window.activeFocusItem.objectName:""});}
  function select():void {files.choose(0,0);files.saveTab();}
  function geometry(name:string):string {const item=fixture.find(name,files.contentItem);if(!item)return "null";const at=item.mapToItem(files.contentItem,0,0);return JSON.stringify({x:at.x,y:at.y,width:item.width,height:item.height});}
 }
}

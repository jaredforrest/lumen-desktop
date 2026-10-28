import QtQuick
import QtQuick.Window
import Quickshell
import Quickshell.Io
import "assets/shell" as D
ShellRoot {id:fixture
 QtObject {id:exported;property var state:({appName:"",title:"",windowId:0})}
 QtObject {id:stub;property int focusEpoch:0;property var windows:[];property var exportedMenu:exported;property date currentDate:new Date(2026,9,20);function activeFiles(window){}function focusWindow(window){}function claimPopup(name){}function focusOwnedWindow(name){}function openSettings(page){}}
 D.FilesState {id:service;path:@FOLDER@;viewMode:"list";persistViewPreferences:true;preferencesLoaded:true}
 D.FilesWindow {id:files;shell:stub;service:service}
 D.CalendarWindow {id:calendar;shell:stub}
 function visit(item,fn){if(fn(item))return item;for(const child of item.children || []){const found=visit(child,fn);if(found)return found;}return null;}
 function facts(window){const fit=visit(window.contentItem,item=>item.ownerWindow===window && item.completedRequests!==undefined);return {visible:window.visible,title:window.title,width:window.width,height:window.height,screen:window.screen?window.screen.name:"",fit:fit?{completed:fit.completedRequests,busy:fit.busy,error:fit.error,result:fit.result}:null};}
 IpcHandler {target:"consumerFit"
  function showConsumer(name:string):void {files.visible=name==="Files";calendar.visible=name==="Calendar";if(name==="Calendar")calendar.open();else service.navigate(service.path,0);}
  function select():void {files.choose(0,0);files.previewOpen=true;files.schedulePreview();}
  function repeat(name:string):void {const window=name==="Files"?files:calendar;for(let i=0;i<5;i++)window.fitScreen();}
  function state(name:string):string {return JSON.stringify({window:fixture.facts(name==="Files"?files:calendar),ready:service.ready && !service.busy && service.rows.length===1,selection:files.selectedPaths,previewOpen:files.previewOpen,preview:service.preview,previewBusy:service.previewBusy,path:service.path,error:service.error});}
 }
}
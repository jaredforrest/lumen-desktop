import QtQuick
import QtQuick.Window
import Quickshell
import Quickshell.Io
import "assets/shell" as D
ShellRoot {
 id:root
 property var fileErrors:[]
 Connections {target:service;function onErrorChanged(){if(service.error)root.fileErrors=root.fileErrors.concat([service.error]);}}
 QtObject {id:exported;property var state:({appName:"",title:"",windowId:0})}
 QtObject {id:stub;property int focusEpoch:0;property var windows:[];property var exportedMenu:exported;property var quickLookWindow:quick;function activeFiles(w){}function focusWindow(w){}function focusOwnedWindow(title){}function claimPopup(owner){}}
 D.FilesState {id:service;persistViewPreferences:false;path:__DOCS_JSON__}
 D.FilesWindow {id:files;shell:stub;service:service;visible:true}
 D.QuickLookWindow {id:quick}
 function find(item,text,seen){if(!item || seen.includes(item))return null;seen.push(item);if((item.text===text || (text==="Private Preview Observer" && item.text===text+" (default)")) && item.visible && item.enabled && item.Window.window && item.Window.window.visible)return item;for(const child of [...(item.children || []),...(item.data || []),...(item.contentData || [])]){const found=find(child,text,seen);if(found)return found;}if(item.contentItem && item.contentItem!==item)return find(item.contentItem,text,seen);return null;}
 function controller(item){if(item.objectName==="owned-preview-state")return item;for(const child of item.children || []){const found=controller(child);if(found)return found;}return null;}
 IpcHandler {target:"fixture"
  function startPreview(path:string):void{quick.show(path,files);}
  function closePreview():void{quick.visible=false;}
  function choose(path:string):void{files.choose(service.rows.findIndex(row=>row.path===path),0);}
  function refresh():void{service.refresh();}
  function applications():void{service.act("applications",{path:files.current.path});}
  function status():string{const c=root.controller(quick.contentItem);return JSON.stringify({result:quick.result,message:quick.actionMessage,filesError:service.error,fileErrors:root.fileErrors,ready:service.ready && !service.busy,current:files.current?files.current.path:null,applications:files.applicationsSelection,actionBusy:c?c.actionBusy:false,retiring:c?c.retiring:false});}
  function button(surface:string,text:string):string{let item=root.find(surface==="quick"?quick.contentItem:files.contentItem,text,[]);if(!item && surface==="quick" && quick.fixtureMenu.visible){for(let i=0;i<quick.fixtureMenu.count;i++){const candidate=quick.fixtureMenu.itemAt(i);if(candidate && candidate.text===text && candidate.enabled)item=candidate;}}if(!item)return "{}";const window=surface==="quick"?quick.contentItem.Window.window:item.Window.window;const point=item.mapToItem(window.contentItem,item.width/2,item.height/2);return JSON.stringify({x:point.x,y:point.y,title:window.title,windowWidth:window.width,windowHeight:window.height,active:window.active || (item.Window.window && item.Window.window.active),activeFocus:item.activeFocus,down:item.down,hovered:item.hovered,pressed:item.pressedSelection});}

 }
 Component.onCompleted:service.navigate(service.path)
}

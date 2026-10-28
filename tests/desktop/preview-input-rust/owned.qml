import QtQuick
import QtQuick.Window
import Quickshell
import Quickshell.Io
import "assets/shell" as D
ShellRoot {
 id:fixture
 property string folder:__DOCS_JSON__
 QtObject {id:exported;property var state:({appName:"",title:"",windowId:0})}
 QtObject {id:stub;property int focusEpoch:0;property var windows:[];property var exportedMenu:exported;property var quickLookWindow:quick;function activeFiles(w){}function focusWindow(w){}function focusOwnedWindow(title){}function claimPopup(owner){}}
 D.FilesState {id:service;persistViewPreferences:false;path:fixture.folder;viewMode:"gallery"}
 D.FilesWindow {id:files;shell:stub;service:service;visible:true;previewOpen:false}
 D.QuickLookWindow {id:quick}
 function previews(item){let found=[];if(item.fixtureHold!==undefined)found.push(item);for(const child of item.children || [])found=found.concat(previews(child));return found;}
 function controller(item){if(item.objectName==="owned-preview-state")return item;for(const child of item.children || []){const found=controller(child);if(found)return found;}return null;}
 function visiblePreview(item){return previews(item).find(p=>p.visible && p.active);}
 IpcHandler {target:"previewAudit"
  function refreshRows():void {service.refresh();}
  function startPreview(surface:string,path:string):void {for(const p of fixture.previews(files.contentItem).concat(fixture.previews(quick.contentItem)))p.fixtureHold=true;if(surface==="quick")quick.show(path,files);else {quick.visible=false;const i=service.rows.findIndex(r=>r.path===path);files.choose(i,0);}}
  function restartFilesPreview(path:string):void {const i=service.rows.findIndex(r=>r.path===path);files.choose(i,0);files.schedulePreview();}
  function closeQuick():void {quick.visible=false;}
  function focusFiles():void {files.focusContents();}
  function release(surface:string):void {const p=surface==="quick"?quick.preview:fixture.visiblePreview(files.contentItem);if(p)p.fixtureHold=false;}
  function pinRetirement(surface:string):void {const p=surface==="quick"?quick.preview:fixture.visiblePreview(files.contentItem);if(!p)return;if(p.result.type==="media")p.fixturePinnedMedia=p.fixtureSource;else p.fixturePinnedImage=p.fixtureSource;}
  function unpinRetirement(surface:string):void {const p=surface==="quick"?quick.preview:fixture.visiblePreview(files.contentItem);if(p){p.fixturePinnedImage="";p.fixturePinnedMedia="";}}
  function page(number:int):void {quick.preview.fixtureHold=true;quick.page=number;quick.refresh();}
  function play():void {quick.preview.togglePlayback();}
  function status(surface:string):string {const p=surface==="quick"?quick.preview:fixture.visiblePreview(files.contentItem),result=surface==="quick"?quick.result:service.preview;return JSON.stringify({ready:service.ready && !service.busy,viewMode:service.viewMode,rows:service.rows.map(r=>({path:r.path,identity:r.identity})),result:result,present:!!p,held:p?p.fixtureHold:false,imageStatus:p?p.fixtureImageStatus:-1,duration:p?p.fixtureDuration:0,playing:p?p.fixturePlaying:false,stopped:p?p.fixtureStopped:true,source:p?p.fixtureSource:"",geometry:p?({title:p.Window.window.title,x:p.mapToItem(p.Window.window.contentItem,0,0).x,y:p.mapToItem(p.Window.window.contentItem,0,0).y,width:p.width,height:p.height}):null,controller:(()=>{const c=fixture.controller(surface==="quick"?quick.contentItem:service);return c?({id:c.currentId,path:c.currentPath,busy:c.busy,retiring:c.retiring,resourceRetirement:c.resourceRetirement.busy,accepted:c.fixtureAccepted}):null;})(),visible:surface==="quick"?quick.visible:files.visible});}
 }
 Component.onCompleted:service.navigate(folder)
}

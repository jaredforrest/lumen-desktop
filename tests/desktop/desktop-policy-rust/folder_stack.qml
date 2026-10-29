import QtQuick
import QtQuick.Window
import Quickshell
import Quickshell.Io
import Quickshell.Wayland
import "assets/shell" as D
ShellRoot {id:fixture
 property string popupOwner:""
 property int focusEpoch:0
 property var windows:ToplevelManager.toplevels.values
 property var ownedContext:null
 property var screenSaverPreferences:null
 property var heldDirectory:null
 property bool holdDirectory:false
 property string reportedError:""
 property var persistedOptions:[]
 property var exportedMenu:exported
 signal dismissPopups(string owner)
 function claimPopup(owner){popupOwner=owner;dismissPopups(owner);}
 function updateFolderStack(path,options){persistedOptions=persistedOptions.concat([{path:path,options:options}]);}
 function reportApplicationError(message){reportedError=message;}
 function focusWindow(window){window.activate();}
 function focusOwnedWindow(title){}
 function activeFiles(window){}
 function newFilesWindow(path){throw Error("Unexpected new window");}
 function openFiles(path,facts){if(holdDirectory)heldDirectory={path:path,facts:facts};else files.openPath(path,facts);}
 QtObject {id:exported;property var state:({appName:"",title:"",windowId:0})}
 D.FilesState {id:service;persistViewPreferences:false}
 D.FilesWindow {id:files;shell:fixture;service:service;visible:false;previewOpen:false}
 D.FolderStack {id:stack;shell:fixture}
 function find(path,item){if(item.visible && ((path==="@footer" && item.objectName==="folder-stack-open-in-files") || (path==="@sort" && item.objectName==="folder-stack-sort") || (path==="@presentation" && item.objectName==="folder-stack-presentation") || (path==="@grid" && item.objectName==="folder-stack-grid") || (item.modelData && item.modelData.path===path && ["folder-stack-list-row","folder-stack-grid-row"].includes(item.objectName))))return item;for(const child of item.children || []){const got=find(path,child);if(got)return got;}return null;}
 IpcHandler {target:"stackAudit"
 function presentFolder(path:string,presentation:string):void{stack.show(path,"HEADLESS-1",{sort:"name",descending:false,presentation:presentation});}
 function reload():void{stack.reload();}
 function closeFiles():void{files.visible=false;}
 function focusControl(path:string):bool{const item=fixture.find(path,stack.contentItem);if(!item)return false;item.forceActiveFocus();return true;}
 function focusFooter():bool{const item=fixture.find("@footer",stack.contentItem);if(!item)return false;item.forceActiveFocus();return true;}
 function hold(value:bool):void{fixture.holdDirectory=value;fixture.heldDirectory=null;}
 function releaseDirectory():void{const captured=fixture.heldDirectory;fixture.heldDirectory=null;if(captured)files.openPath(captured.path,captured.facts);}
 function choose(index:int):void{stack.selected=index;}
 function holdListing(value:bool):void{stack.fixtureHoldListing=value;}
 function killListing():bool{if(!stack.fixtureListing.running || stack.fixtureListing.received)return false;stack.fixtureListing.signal(15);return true;}
 function status():string{return JSON.stringify({visible:stack.visible,folder:stack.folder,rowCount:stack.rows.length,rows:stack.rows.slice(0,30).map(r=>({path:r.path,name:r.name,directory:r.directory,facts:r.actionFacts})),facts:stack.folderFacts,error:stack.error,busy:stack.actionBusy,listing:stack.fixtureListing.running,generation:stack.generation,sortPopup:fixture.find("@sort",stack.contentItem)?fixture.find("@sort",stack.contentItem).popup.visible:false,scrollY:fixture.find("@grid",stack.contentItem)?fixture.find("@grid",stack.contentItem).contentY:0,sort:stack.sort,presentation:stack.presentation,persisted:fixture.persistedOptions,grid:stack.grid,limited:stack.limited,total:stack.total,held:fixture.heldDirectory,filesVisible:files.visible,filesBusy:service.busy,filesPath:service.path,filesRows:service.rows.map(r=>r.name),filesError:service.error,filesHistory:service.history.length,reported:fixture.reportedError});}
 function point(path:string):string{const item=fixture.find(path,stack.contentItem);if(!item)return "null";const at=item.mapToItem(stack.contentItem,0,0);return JSON.stringify({x:at.x,y:at.y,width:item.width,height:item.height,hovered:item.hovered,down:item.down,active:item.activeFocus});}
 }
}

import QtQuick
import QtQuick.Window
import Quickshell
import Quickshell.Io
import "assets/shell" as D
ShellRoot {
 id:fixture
 D.OwnedWindowContext {id:context}
 QtObject {id:exported;property var state:({appName:"",title:"",windowId:0})}
 QtObject {id:stub;property int focusEpoch:0;property var windows:[];property var exportedMenu:exported;property var ownedContext:context;property var filesWindow:first;function activeFiles(window){filesWindow=window;}function focusWindow(window){}function focusOwnedWindow(title){Quickshell.execDetached(["swaymsg",'[title="^'+title+'$"] focus']);} }
 D.FilesState {id:firstService;persistViewPreferences:false;path:@FOLDER@;viewMode:"list"}
 D.FilesState {id:secondService;persistViewPreferences:false;path:@FOLDER@;viewMode:"list"}
 D.FilesWindow {id:first;shell:stub;service:firstService;visible:true}
 D.FilesWindow {id:second;shell:stub;service:secondService;windowNumber:2;visible:false}
 D.QuickLookWindow {id:preview}
 FloatingWindow {id:other;title:"Unrelated fixture";visible:false;implicitWidth:240;implicitHeight:160;Rectangle{id:otherBody;anchors.fill:parent;color:"#405060"}}
 Component.onCompleted:{firstService.navigate(firstService.path);secondService.navigate(secondService.path);}
 IpcHandler {target:"ownershipAudit"
  function state():string{return JSON.stringify({ready:firstService.ready && !firstService.busy && firstService.rows.length>0,main:context.mainWindow===first?"first":context.mainWindow===second?"second":"",key:context.keyWindow===first?"first":context.keyWindow===second?"second":context.keyWindow===preview?"preview":context.keyWindow===first.informationWindow?"info":(second && context.keyWindow===second.informationWindow)?"secondInfo":"",role:context.keyRole,app:context.applicationId,menuTarget:stub.filesWindow===first?"first":"second",firstActive:first.selectionActive,previewActive:preview.keyActive,infoActive:first.informationWindow.keyActive,otherActive:otherBody.Window.window?otherBody.Window.window.active:false,firstTabs:first.tabs.length,secondTabs:second.tabs.length});}
  function firstFocus():void{first.focusContents();}
  function previewFocus():void{preview.show(@DOCUMENT@,first);}
  function infoFocus():void{first.informationWindow.show({name:"Synthetic document",path:@DOCUMENT@});}
  function secondInfoFocus():void{second.informationWindow.show({name:"Second synthetic document"});}
  function otherFocus():void{other.visible=true;Qt.callLater(()=>otherBody.Window.window.requestActivate());}
  function secondFocus():void{second.openPath(@FOLDER@);}
  function commandNewTab():void{stub.filesWindow.newTab();}
  function closeFirst():void{first.visible=false;}
  function closePreview():void{preview.returnTo=null;preview.visible=false;}
 }
}
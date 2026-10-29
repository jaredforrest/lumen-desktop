
 property var context:ownedWindowContext
 property var first:primaryFiles
 property var firstService:primaryFilesState
 property var second:filesInstances.length?filesInstances[0].window:null
 property var preview:quickLook
 property var stub:root
 FloatingWindow {id:other;title:"Unrelated fixture";visible:false;implicitWidth:240;implicitHeight:160;Rectangle{id:otherBody;anchors.fill:parent;color:"#405060"}}
 Component.onCompleted:root.openFiles(@FOLDER@);
 IpcHandler {target:"ownershipAudit"
  function state():string{return JSON.stringify({filesGroupTitles:root.windowsFor(DesktopEntries.byId("lumen-files")).map(w=>w.title),group:root.active && root.entryFor(root.active)?root.entryFor(root.active).id:"",ready:firstService.ready && !firstService.busy && firstService.rows.length>0,main:context.mainWindow===first?"first":(second && context.mainWindow===second)?"second":"",key:context.keyWindow===first?"first":(second && context.keyWindow===second)?"second":context.keyWindow===preview?"preview":context.keyWindow===first.informationWindow?"info":(second && context.keyWindow===second.informationWindow)?"secondInfo":"",role:context.keyRole,app:context.applicationId,menuTarget:stub.filesWindow===first?"first":"second",firstActive:first.selectionActive,previewActive:preview.keyActive,infoActive:first.informationWindow.keyActive,otherActive:otherBody.Window.window?otherBody.Window.window.active:false,firstTabs:first.tabs.length,secondTabs:second?second.tabs.length:1});}
  function firstFocus():void{first.focusContents();}
  function previewFocus():void{preview.show(@DOCUMENT@,first);}
  function infoFocus():void{first.informationWindow.show({name:"Synthetic document",path:@DOCUMENT@});}
  function secondInfoFocus():void{second.informationWindow.show({name:"Second synthetic document"});}
  function otherFocus():void{other.visible=true;Qt.callLater(()=>otherBody.Window.window.requestActivate());}
  function secondFocus():void{root.newFilesWindow(@FOLDER@);}
  function commandNewTab():void{root.filesAction("new-tab");}
  function closeFirst():void{first.visible=false;}
  function closePreview():void{preview.returnTo=null;preview.visible=false;}
 }

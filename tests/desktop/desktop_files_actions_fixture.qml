import QtQuick
import Quickshell
ShellRoot {
 id:root
 property int ticks:0
 QtObject {id:quick;function show(path,owner){}}
 QtObject {id:stub;property var quickLookWindow:quick;property var opened:[];function openFiles(path){opened=opened.concat([path]);}}
 QtObject {id:service
  property var rows:[{name:"Folder",path:"/fixture/Folder",directory:true,link:false},{name:"document.txt",path:"/fixture/document.txt",directory:false,link:false}]
  property bool writable:true
  property string error:""
  property string warning:""
  property var calls:[]
  signal informationReady(var details)
  function record(action,data){calls=calls.concat([{action:action,data:data}]);}
  function startJob(operation,paths){record(operation,paths);}
  function act(action,data){record(action,data);}
  function send(action,data){record(action,data);}
  function stage(paths,operation){record(operation,paths);}
  function paste(){record("paste",{});}
  function undo(){record("undo",{});}
  function refresh(){record("refresh",{});}
  function navigate(path){record("navigate",path);}
 }
 FloatingWindow {visible:true;implicitWidth:1200;implicitHeight:900
  Loader {id:loader;anchors.fill:parent;Component.onCompleted:setSource("LUMEN_ASSETS/DesktopFiles.qml",{shell:stub,service:service,resolveDirectory:false,directory:"/fixture",active:true})}
 }
 function check(value,message){if(!value)throw Error(message);}
 Timer {interval:100;running:true;repeat:true;onTriggered:{
  try {
   if(++root.ticks>60)throw Error("Desktop files timed out loading");
   if(!loader.item)return;
   const desktop=loader.item;
   desktop.choose(0,0);desktop.openCurrent();check(stub.opened[0]==="/fixture/Folder","Folder was not opened in Files");
   desktop.choose(1,0);desktop.openCurrent();check(service.calls[0].action==="open" && service.calls[0].data.path==="/fixture/document.txt","File was not opened through the shared service");
   desktop.choose(0,0);desktop.choose(1,Qt.ControlModifier);check(desktop.selectedPaths.length===2,"Multiple selection failed");
   desktop.askTrash();check(desktop.confirmationVisible && service.calls.length===1,"Trash ran before confirmation");
   desktop.active=false;check(!desktop.confirmationVisible && service.calls.length===1,"Dismissal executed a destructive action");
   desktop.active=true;desktop.choose(1,0);desktop.askTrash();desktop.confirmTrash();
   check(service.calls.length===2 && service.calls[1].action==="trash" && service.calls[1].data[0]==="/fixture/document.txt","Confirmed trash lost the selected intent");
   service.rows=[];check(desktop.selectedPaths.length===0 && desktop.currentPath==="","Removed files retained stale selection");
   console.log("DESKTOP_FILES_PASS");Qt.quit();
  }catch(error){console.error("DESKTOP_FILES_FAIL "+error);Qt.quit();}
 }}
}

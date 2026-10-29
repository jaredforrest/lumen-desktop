import QtQuick
import Quickshell
import "LUMEN_ASSETS" as D
ShellRoot {
 id:fixture
 property int phase:0
 property int ticks:0
 property var selected:[{path:"/synthetic/one",identity:{device:"1",inode:"1152921504606846977"}},{path:"/synthetic/two",identity:{device:"1",inode:"1152921504606846978"}}]
 QtObject {id:service
  property bool ready:true
  property int sequence:0
  property var requests:[]
  signal batchRenameReady(var reply)
  function send(action,data){const id=++sequence;requests=requests.concat([{id:id,action:action,data:data}]);return id;}
 }
 FloatingWindow {id:window;visible:true;implicitWidth:800;implicitHeight:700
  D.BatchRenameDialog {id:dialog;service:service;ownerWidth:window.width;ownerHeight:window.height}
 }
 function check(value,message){if(!value)throw Error(message+" phase="+phase);}
 function find(name,item){if(item.objectName===name)return item;for(const child of item.children || []){const found=find(name,child);if(found)return found;}return null;}
 function previewReply(id,token){service.batchRenameReady({id:id,action:"rename-preview",token:token,rows:[{before:"one",after:"new-one",status:"ready",error:""},{before:"two",after:"new-two",status:"ready",error:""}],executable:true,count:2});}
 Timer {interval:50;running:true;repeat:true;onTriggered:{try {
  if(++ticks>100)throw Error("Dialog protocol timeout");
  if(phase===0){dialog.begin(selected);dialog.preview();previewReply(service.requests[service.requests.length-1].id,"entering-plan");check(dialog.token==="entering-plan" && dialog.executable,"Legitimate reply during popup entry discarded");dialog.changed();phase=1;return;}
  if(phase===1){check(dialog.opened,"Dialog did not open");find("batch-rename-first",dialog.contentItem).text="new-";dialog.preview();const request=service.requests[service.requests.length-1];check(request.action==="rename-preview","Missing preview");check(request.data.items[0].identity.inode==="1152921504606846977","Identity precision lost");selected[0].identity.inode="99";selected[0].path="/synthetic/bystander";check(dialog.captured[0].identity.inode==="1152921504606846977" && dialog.captured[0].path==="/synthetic/one","Selection was not captured");previewReply(request.id,"captured-plan");phase=2;return;}
  if(phase===2){check(dialog.executable,"Preview cannot be confirmed");dialog.confirm();dialog.confirm();const confirms=service.requests.filter(request=>request.action==="rename-confirm");check(confirms.length===1 && confirms[0].data.token==="captured-plan" && confirms[0].data.confirmed===true,"Confirmation duplicated or retargeted");service.batchRenameReady({id:confirms[0].id,action:"rename-confirm",ok:false,rows:[{before:"one",after:"new-one",status:"renamed",error:""},{before:"two",after:"new-two",status:"failed",error:"Permission denied"}]});phase=3;return;}
  if(phase===3){check(dialog.completed && !dialog.executing && dialog.rows[1].status==="failed","Partial result hidden");dialog.close();dialog.begin(selected);find("batch-rename-first",dialog.contentItem).text="new-";dialog.preview();const request=service.requests[service.requests.length-1];dialog.changed();previewReply(request.id,"stale-plan");check(!dialog.executable && dialog.token==="","Changed form accepted old preview");check(service.requests.some(request=>request.action==="rename-cancel" && request.data.token==="stale-plan"),"Late preview token leaked");phase=4;return;}
  if(phase===4){dialog.preview();const request=service.requests[service.requests.length-1];dialog.close();previewReply(request.id,"closed-plan");check(service.requests.some(request=>request.action==="rename-cancel" && request.data.token==="closed-plan"),"Closed preview token leaked");service.ready=false;check(dialog.token==="" && !dialog.executable,"Disconnect retained authority");console.log("BATCH_RENAME_QT_PASS");Qt.quit();}
 }catch(error){console.error("BATCH_RENAME_QT_FAIL "+error);Qt.quit();}}}
}
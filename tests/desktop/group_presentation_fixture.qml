import QtQuick
import Quickshell
import "LUMEN_ASSETS" as D
ShellRoot {
 D.WindowGroupState {id:group}
 property int phase:0
 property int budget:0
 function check(value,message){if(!value)throw Error(message);}
 function state(epoch,revision,supported,locked){return {available:true,enabled:false,presentationSupported:supported,locked:locked,epoch:epoch,revision:revision,windowMode:"all_together",outputs:[]};}
 function desired(value){D.Theme.windowsPreferences=Object.assign({},D.Theme.windowsPreferences,{groupWindowMode:value});}
 Component.onCompleted:group.observer.running=false
 Timer {interval:10;running:true;repeat:true;onTriggered:{group.observer.running=false;group.retry.stop();}}
 Timer {interval:30;running:true;repeat:true;onTriggered:{try{
  if(++budget>250)throw Error("bounded group transport deadline at "+phase);
  if(phase===0){group.retry.stop();desired("one_at_a_time");group.state=state("old",1,false,false);group.reconcileMode();check(!group.modeWorker.running,"unsupported provider dispatched");group.state=state("locked",2,true,true);group.reconcileMode();check(!group.modeWorker.running,"locked provider dispatched");group.state=state("owned",7,true,false);group.reconcileMode();phase++;return;}
  if(phase===1){if(group.modeWorker.running || !group.modeWorker.received)return;check(!group.presentationError,"valid mode failed");check(group.modeRequest.epoch==="owned" && group.modeRequest.revision===7 && group.modeRequest.windowMode==="one_at_a_time","capture changed");group.reconcileMode();check(!group.modeWorker.running,"unchanged observation retried");group.state=state("stale",8,true,false);group.reconcileMode();phase++;return;}
  if(phase===2){if(group.modeWorker.running || !group.modeWorker.received)return;check(group.presentationError.includes("changed"),"stale receipt not surfaced");group.reconcileMode();check(!group.modeWorker.running,"stale intent blindly retried");group.state=state("malformed",9,true,false);group.reconcileMode();phase++;return;}
  if(phase===3){if(group.modeWorker.running || !group.modeWorker.received)return;check(group.presentationError.includes("Invalid"),"malformed receipt not surfaced");group.modeDeadline.interval=80;group.modeKill.interval=50;group.state=state("stall",10,true,false);group.reconcileMode();phase++;return;}
  if(phase===4){if(group.modeWorker.running)return;check(group.modeWorker.timedOut && group.presentationError.includes("timed out"),"stalled owner not retired truthfully");group.reconcileMode();check(!group.modeWorker.running,"stalled capture retried");group.modeDeadline.interval=15000;group.state=state("fresh",11,true,false);group.reconcileMode();phase++;return;}
  if(group.modeWorker.running || !group.modeWorker.received)return;
  check(!group.presentationError,"fresh capture did not recover");group.state=Object.assign({},group.state,{windowMode:"one_at_a_time"});group.reconcileMode();check(!group.modeWorker.running,"already applied mode dispatched");console.log("GROUP_PRESENTATION_QML_PASS");Qt.quit();
 }catch(error){console.error("GROUP_PRESENTATION_QML_FAIL "+error);Qt.quit();}}}
}

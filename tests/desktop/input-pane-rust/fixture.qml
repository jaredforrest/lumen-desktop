import QtQuick
import Quickshell
import "@SHELL_URI@" as D
ShellRoot {
 id:test;property int step:0
 D.InputSettingsState {id:service;active:true}
 D.InputSettingsPane {id:pane;width:240;service:service}
 function find(name,item){if(item.objectName===name)return item;for(const child of item.children || []){const found=find(name,child);if(found)return found;}return null;}
 Timer {interval:100;running:true;repeat:true;onTriggered:{try {
  if(service.busy)return;
  if(!service.available || service.error)throw Error("Provider "+service.error);
  if(test.step===0){
   pane.selected=service.devices.find(row=>row.identity.type==="pointer").key;test.step++;return;
  }
  if(test.step===1){
   const toggle=find("input-option-natural_scroll",pane);if(!toggle || !toggle.enabled || toggle.checked)throw Error("Wrong selected capability");
   toggle.checked=true;toggle.clicked();test.step++;return;
  }
  if(test.step===2){
   const option=pane.device.controls.find(row=>row.key==="natural_scroll");if(!option.value || !option.saved || !service.message)throw Error("Save not confirmed");
   const restore=find("input-restore-natural_scroll",pane);if(!restore.visible || !restore.enabled)throw Error("No reversal");restore.clicked();test.step++;return;
  }
  if(test.step===3){
   if(pane.device.controls.find(row=>row.key==="natural_scroll").value)throw Error("Restore not applied");
   pane.selected=service.devices.find(row=>row.identity.type==="touchpad").key;test.step++;return;
  }
  if(test.step===4){
   const toggle=find("input-option-tap",pane);if(!toggle || !toggle.enabled)throw Error("Trackpad tap absent");toggle.checked=true;toggle.clicked();test.step++;return;
  }
  if(test.step===5){
   if(!pane.device.controls.find(row=>row.key==="tap").value)throw Error("Tap not applied");
   find("input-forget-profile",pane).clicked();
   if(pane.forgetTarget.identity.identifier!==pane.device.identity.identifier || service.busy)throw Error("Forget changed before confirmation");
   find("input-forget-cancel",pane).clicked();if(pane.forgetTarget!==null)throw Error("Forget cancel failed");
   console.log("INPUT_SETTINGS_PASS");Qt.quit();
  }
 }catch(error){console.error("INPUT_SETTINGS_FAIL "+error);Qt.quit();}}}
}

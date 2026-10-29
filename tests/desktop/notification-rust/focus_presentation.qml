import QtQuick
import QtQuick.Window
import Quickshell
import "assets/shell" as D
ShellRoot {
 id:test
 property int phase:0
 property int ticks:0
 property bool capturePending:false
 QtObject {id:testShell;property var bars:({});function preferredOutput(){return "";}function claimPopup(value){}}
 D.NotificationState {id:service;shell:testShell}
 Window {id:window;visible:true;width:360;height:110;color:D.Theme.colors.background
  Item {id:frame;width:360;height:110
   Rectangle {anchors.fill:parent;color:D.Theme.colors.background}
   D.NotificationCentre {id:centre;width:320;x:20;y:20;service:service}
  }
 }
 function snapshot(name,next){
  const labels=centre.children.filter(child=>child.visible && child.text!==undefined).map(child=>String(child.text));
  const expected=name==="active"?"Do Not Disturb is on\nNo notifications":"No notifications";
  if(labels.length!==1 || labels[0]!==expected)throw Error("painted centre label mismatch "+JSON.stringify(labels));
  capturePending=true;
  if(!frame.grabToImage(result=>{
   if(!result.saveToFile(Quickshell.env("LUMEN_FOCUS_CAPTURE_DIR")+"/"+name+".png")){console.error("EXPIRY_FAIL capture write");Qt.exit(1);return;}
   console.log("EXPIRY_FRAME "+JSON.stringify({name:name,label:labels[0],focus:service.focusState,dnd:service.dnd,revision:service.revision}));
   test.capturePending=false;test.phase=next;
  },Qt.size(360,110)))throw Error("capture refused");
 }
 Timer {interval:50;repeat:true;running:true;onTriggered:{try{
  if(++test.ticks>160)throw Error("Focus expiry presentation deadline");
  if(test.capturePending)return;
  if(test.phase===0 && service.available){if(service.dnd || service.focusState.active!=="")throw Error("fresh daemon unexpectedly focused");service.command({action:"focus",profile:"quiet",seconds:2});test.phase=1;return;}
  if(test.phase===1 && service.dnd && service.focusState.active==="quiet"){test.snapshot("active",2);return;}
  if(test.phase===2 && !service.dnd && service.focusState.active==="" && service.focusState.scheduled===true){test.snapshot("expired",3);return;}
  if(test.phase===3){console.log("EXPIRY_PASS");Qt.quit();}
 }catch(error){console.error("EXPIRY_FAIL "+error);Qt.exit(1);}}}
}

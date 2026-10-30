import QtQuick
import Quickshell

ShellRoot {
 id:test
 property string mode:Quickshell.env("LUMEN_NATIVE_FOCUS_MODE")
 property int stage:0
 property int ticks:0
 property int began:0
 property string owned:""
 property var receipts:[]
 property bool failed:false
 function check(ok,label){if(!ok){failed=true;console.error("FAIL native transport "+mode+": "+label);Qt.quit();}return ok;}
 ApplicationLifecycleState {
  id:state
  onNativeFocusResult:result=>{
   if(!test.check(result.requestId===test.owned,"exact owned request correlation"))return;
   if(!test.check(typeof result.requested==="boolean","boolean completion"))return;
   test.receipts=test.receipts.concat([result]);
   if(test.receipts.length===1 && test.mode!=="success"){
    if(!test.check(result.requested===false && result.uncertain===true,"uncertain cancellation"))return;
    const serial=state.nativeSerial;
    test.check(state.focusNative({},null,true)==="" && state.nativeSerial===serial,"retiring request blocks replacement dispatch");
   }else test.check(result.requested===true,"successful exact owned reply");
  }
 }
 Component.onCompleted:{if(state.nativeDeadline)state.nativeDeadline.interval=200;state.retry.interval=25;}
 Timer {interval:10;running:true;repeat:true;onTriggered:{
  if(test.failed)return;
  ++test.ticks;
  if(test.ticks>450){test.check(false,"bounded completion deadline at stage "+test.stage);return;}
  if(test.stage===0 && state.available){test.owned=state.focusNative({},null,true);test.check(!!test.owned,"initial owned dispatch");test.stage=1;}
  else if(test.stage===1 && test.receipts.length===1){test.began=test.ticks;test.stage=2;}
  else if(test.stage===2 && test.ticks-test.began>10 && state.available && !state.nativeRetiring){
   test.check(!state.nativePending,"completed transport pending flag");test.owned=state.focusNative({},null,true);test.check(!!test.owned,"explicit fresh request after recovery");test.stage=3;
  }else if(test.stage===3 && test.receipts.length===2){test.began=test.ticks;test.stage=4;}
  else if(test.stage===4 && test.ticks-test.began>10){
   if(!test.check(test.receipts.length===2 && !state.nativePending && !state.nativeRetiring,"unsolicited idle reply ignored"))return;
   console.log("PASS native focus transport "+test.mode);Qt.quit();
  }
 }}
}

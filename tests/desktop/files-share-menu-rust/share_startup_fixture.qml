import QtQuick
import Quickshell
import "assets/shell" as D
ShellRoot {
 id:test
 property int phase:0
 property int ticks:0
 property int terminalSignals:0
 property int settledTicks:0
 property var events:[]
 D.ShareDestinationsState {id:state;onFinished:result=>{test.terminalSignals++;test.events.push("finished:"+result.state);}}
 Connections {target:state.reader;function onExited(code,status){test.events.push("reader-exited:"+code);}function onRunningChanged(){test.events.push("reader-running:"+state.reader.running);}}
 Connections {target:state.invoker;function onExited(code,status){test.events.push("invoker-exited:"+code);}function onRunningChanged(){test.events.push("invoker-running:"+state.invoker.running);}}
 function check(value,message){if(!value)throw Error(message);}
 Timer {interval:40;running:true;repeat:true;onTriggered:{try{
  if(++test.ticks>50)throw Error("Startup failure readiness deadline");
  if(test.phase===0){state.refresh();check(state.loading,"reader synchronous latch");test.phase=1;}
  else if(test.phase===1){if(state.loading)return;check(state.error.includes("could not start") && !state.destinations.length,"failed reader retires with actionable error");state.invoke({});check(state.working,"invoker synchronous latch");test.phase=2;}
  else if(test.phase===2){if(state.working)return;check(state.outcome && state.outcome.state==="error" && test.terminalSignals===1,"failed invocation has one truthful terminal signal");if(++test.settledTicks<4)return;console.log("SHARE_STARTUP_PASS "+JSON.stringify({checks:4,terminalSignals:test.terminalSignals,events:test.events}));Qt.quit();}
 }catch(error){console.error("SHARE_STARTUP_FAIL "+error);Qt.exit(1);}}}
}

import QtQuick
import Quickshell
import "LUMEN_ASSETS" as D
ShellRoot {
 QtObject {id:shell;property var windowGroupPresentation:({available:true,locked:false,epoch:"group-epoch",revision:1})}
 QtObject {id:owner;property var shell:shell;property bool visible:true;property bool busy:false;property string sessionEpoch:"overview-epoch";property int revision:6}
 D.DesktopWindowDrag {id:drag;owner:owner}
 property int phase:0
 property int budget:0
 function check(value,message){if(!value)throw Error(message);}
 function captured(foreign){return {sessionEpoch:"overview-epoch",revision:6,snapshot:{digest:"frozen"},source:{id:7,pid:23,workspace:4,output:2,foreign:foreign},destinations:[{logicalId:"desktop-b",workspace:12,output:2,name:"two"}]};}
 function commit(foreign){check(drag.begin(captured(foreign),false),"begin failed");drag.commit(drag.destinations[0]);}
 Timer {interval:30;running:true;repeat:true;onTriggered:{try{
  if(++budget>250)throw Error("bounded transport deadline "+phase);
  if(phase===0){
   const unsafe=captured("valid");unsafe.source.id=9007199254740992;check(!drag.begin(unsafe,false),"unsafe native integer offered");
   shell.windowGroupPresentation={available:true,locked:true,epoch:"group-epoch"};check(!drag.begin(captured("valid"),false),"locked capture began");
   shell.windowGroupPresentation={available:true,locked:false,epoch:"group-epoch",revision:1};
   check(drag.begin(captured("valid"),false),"preview failed");check(!drag.busy && drag.request===null,"preview dispatched");check(!drag.accepts({getDataAsString:function(){return "forged";}}),"forged token accepted");
   shell.windowGroupPresentation={available:true,locked:false,epoch:"group-epoch",revision:2};check(drag.active,"different epoch namespaces cancelled valid drag");drag.cancel();
   check(drag.begin(captured("valid"),false),"second preview failed");owner.revision=7;drag.commit(drag.destinations[0]);check(!drag.active && !drag.busy,"stale owner committed");owner.revision=6;
   check(drag.begin(captured("valid"),false),"epoch preview failed");shell.windowGroupPresentation={available:true,locked:false,epoch:"new-group"};check(!drag.active,"native session change did not cancel");
   shell.windowGroupPresentation={available:true,locked:false,epoch:"group-epoch"};
   const value=captured("valid");check(drag.begin(value,false),"capture failed");value.source.id=999;value.destinations[0].name="substituted";drag.commit(drag.destinations[0]);phase++;return;
  }
  if(phase===1){if(drag.busy || !drag.worker.received)return;check(drag.commits===1 && !drag.error && drag.request.source.id===7 && drag.request.destination.name==="two","immutable confirmation failed");commit("stale");phase++;return;}
  if(phase===2){if(drag.busy || !drag.worker.received)return;check(drag.commits===1 && drag.error.includes("changed"),"stale error lost");check(drag.begin(captured("valid"),false),"lock preview failed");shell.windowGroupPresentation={available:true,locked:true,epoch:"group-epoch"};check(!drag.active && !drag.busy,"lock did not cancel");shell.windowGroupPresentation={available:true,locked:false,epoch:"group-epoch"};commit("malformed");phase++;return;}
  if(phase===3){if(drag.busy || !drag.worker.received)return;check(drag.error.includes("valid confirmation") && drag.commits===1,"malformed result not surfaced");drag.deadline.interval=80;drag.killDelay.interval=50;commit("stall");check(!drag.begin(captured("valid"),false),"running owner replaced");phase++;return;}
  if(phase===4){if(drag.busy)return;check(drag.worker.timedOut && drag.error.includes("timed out") && drag.commits===1,"uncertain owner not retired truthfully");drag.deadline.interval=15000;commit("fresh");phase++;return;}
  if(drag.busy || !drag.worker.received)return;check(drag.commits===2 && !drag.error,"fresh independent intent did not recover");console.log("DESKTOP_WINDOW_TRANSFER_PASS");Qt.quit();
 }catch(error){console.error("DESKTOP_WINDOW_TRANSFER_FAIL "+error);Qt.quit();}}}
}

import QtQuick
import Quickshell
import "LUMEN_ASSETS" as D
ShellRoot {
 id:fixture
 property int refreshes:0
 property int phase:0
 property int ticks:0
 property int lastListing:0
 D.FilesState {id:service;persistViewPreferences:false;function refresh(){fixture.refreshes++;}}
 D.FilesState {id:live;persistViewPreferences:false;path:"/fixture";viewMode:"list"}
 function check(value,message){if(!value)throw new Error(message);}
 function receipt(id,state,extra){return Object.assign({id:id,state:state,operation:"undo",errors:[],started:Date.now()+1000},extra || {});}
 function accept(job){service.acceptJobs({jobs:[job],canUndo:false,undoKind:null,undoLabel:"Undo Last Move"});}
 Timer {interval:1;running:true;repeat:false;onTriggered:{try {
  fixture.refreshes=0;
  const normal=fixture.receipt("normal","complete",{recoveryPath:"/private/Trash/files/token",undoMessage:"Copy undone. Use Put Back."});
  service.error="Previous notice";fixture.accept(normal);
  fixture.check(service.error==="" && service.warning==="","Ordinary success displayed as an error or warning");
  fixture.accept(normal);fixture.check(service.error==="","Polling reintroduced a success recovery path");
  fixture.accept(fixture.receipt("failed","failed",{errors:["Could not finish"],recoveryPath:"/private/held/data",undoMessage:"Retained item requires recovery"}));
  fixture.check(service.error.includes("Retained item") && service.error.includes("/private/held/data"),"Failed retained-data recovery was hidden");
  service.error="";fixture.accept(fixture.receipt("ineligible","complete",{operation:"copy",undoWarning:"This copy has no Undo; earlier named history remains"}));
  fixture.check(service.error==="" && service.operationWarning.includes("This copy has no Undo") && service.visibleWarning.includes("This copy has no Undo"),"Successful ineligible Copy was marked as failed");
  fixture.accept(fixture.receipt("interrupted","interrupted",{errors:["Inspect both locations"],recoveryPath:"/private/stage/RECOVERY.txt",undoMessage:"Interrupted; inspect recovery record"}));
  fixture.check(service.error.includes("/private/stage/RECOVERY.txt"),"Interrupted retained-data path was hidden");
  fixture.check(fixture.refreshes===4,"Receipt polling refreshed repeatedly or missed transitions");
  const recovered=service.jobs[0],next=fixture.receipt("next-undo","complete",{recoveryPath:"/private/Trash/files/next",undoMessage:"Copy undone. Use Put Back."});
  service.acceptJobs({jobs:[recovered,next],canUndo:false});
  fixture.check(service.error==="","Previously disclosed interrupted history overwrote new Undo success");
  service.acceptJobs({jobs:[recovered,next],canUndo:false});
  fixture.check(service.error==="" && fixture.refreshes===5,"Polling replayed a historical recovery error");
  const old=fixture.receipt("older-than-startup","interrupted",{started:service.createdAt-1,errors:["Interrupted"],recoveryPath:"/private/older/RECOVERY.txt",undoMessage:"Inspect retained item"});
  service.error="";fixture.accept(old);
  fixture.check(service.error.includes("/private/older/RECOVERY.txt"),"Startup recovery from before this UI session was hidden");
  const older=fixture.receipt("unseen-older","interrupted",{started:service.createdAt-10,errors:["Old"],recoveryPath:"/private/old/data",undoMessage:"Old recovery"});
  const newer=fixture.receipt("unseen-newer","failed",{started:Date.now()+10000,errors:["New"],recoveryPath:"/private/new/data",undoMessage:"Newer recovery"});
  service.acceptJobs({jobs:[newer,older],canUndo:false});
  fixture.check(service.error.includes("/private/new/data") && !service.error.includes("/private/old/data"),"Startup fallback overrode newer terminal recovery");
  fixture.phase=1;live.send("fixture-warning");
 }catch(error){console.error("FILES_JOB_RECEIPTS_QT_FAIL "+error);Qt.quit();}}}
 Timer {interval:10;running:fixture.phase>0;repeat:true;onTriggered:{try {
  fixture.ticks++;fixture.check(fixture.ticks<400,"Live job/list transport deadline");
  fixture.check(live.error==="","Actual transport reported "+live.error);
  if(live.busy || live.committedListingId<=fixture.lastListing)return;
  if(fixture.phase===1){
   fixture.check(live.operationWarning.includes("This copy has no Undo") && live.warning==="" && live.visibleWarning.includes("This copy has no Undo"),"Clean listing erased operation warning");
   fixture.lastListing=live.committedListingId;fixture.phase=2;live.refresh();return;
  }
  if(fixture.phase===2){
   fixture.check(live.warning.includes("5,000") && live.visibleWarning.includes("5,000") && live.visibleWarning.includes("This copy has no Undo"),"Listing constraint or operation warning was hidden");
   fixture.lastListing=live.committedListingId;fixture.phase=3;live.refresh();return;
  }
  fixture.check(live.warning==="" && live.operationWarning.includes("This copy has no Undo") && live.visibleWarning.includes("This copy has no Undo"),"Later clean listing erased durable operation warning");
  console.log("FILES_JOB_RECEIPTS_QT_PASS");Qt.quit();
 }catch(error){console.error("FILES_JOB_RECEIPTS_QT_FAIL "+error);Qt.quit();}}}
}

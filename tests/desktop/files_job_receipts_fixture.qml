import QtQuick
import Quickshell
import "LUMEN_ASSETS" as D
ShellRoot {
 id:fixture
 property int refreshes:0
 D.FilesState {id:service;persistViewPreferences:false;function refresh(){fixture.refreshes++;}}
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
  fixture.check(service.error==="" && service.warning.includes("This copy has no Undo"),"Successful ineligible Copy was marked as failed");
  fixture.accept(fixture.receipt("interrupted","interrupted",{errors:["Inspect both locations"],recoveryPath:"/private/stage/RECOVERY.txt",undoMessage:"Interrupted; inspect recovery record"}));
  fixture.check(service.error.includes("/private/stage/RECOVERY.txt"),"Interrupted retained-data path was hidden");
  fixture.check(fixture.refreshes===4,"Receipt polling refreshed repeatedly or missed transitions");
  const recovered=service.jobs[0],next=fixture.receipt("next-undo","complete",{recoveryPath:"/private/Trash/files/next",undoMessage:"Copy undone. Use Put Back."});
  service.acceptJobs({jobs:[recovered,next],canUndo:false});
  fixture.check(service.error==="","Previously disclosed interrupted history overwrote new Undo success");
  service.acceptJobs({jobs:[recovered,next],canUndo:false});
  fixture.check(service.error==="" && fixture.refreshes===5,"Polling replayed a historical recovery error");
  console.log("FILES_JOB_RECEIPTS_QT_PASS");Qt.quit();
 }catch(error){console.error("FILES_JOB_RECEIPTS_QT_FAIL "+error);Qt.quit();}}}
}

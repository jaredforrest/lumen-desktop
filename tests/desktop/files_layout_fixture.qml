import QtQuick
import Quickshell
import "LUMEN_ASSETS" as D
ShellRoot {
 id:fixture
 property int phase:0
 property int ticks:0
 property string firstId:""
 property string secondId:""
 property string thirdId:""
 property var thirdHistory:[]
 property int thirdHistoryIndex:-1
 property string base:"LUMEN_FILES_BASE"
 QtObject {id:exported;property var state:({appName:"",title:"",windowId:0})}
 QtObject {id:stub;property int focusEpoch:0;property var windows:[];property var exportedMenu:exported;function activeFiles(window){}function focusWindow(window){}}
 D.FilesState {id:service;persistViewPreferences:false;path:fixture.base+"/First"}
 D.FilesWindow {id:files;shell:stub;service:service;visible:true}
 function check(value,message){if(!value)throw Error(message+" phase="+phase);}
 function select(){files.choose(0,0);check(files.selectedPaths.length===1,"Selection missing");}
 Timer {interval:60;running:true;repeat:true;onTriggered:{try {
  if(++ticks>240)throw Error("Files layout timeout phase="+phase+" busy="+service.busy+" error="+service.error);
  if(!service.ready || service.busy)return;
  if(phase===0){service.navigate(base+"/First");phase=1;return;}
  if(!service.rows.length)return;
  if(phase===1){select();firstId=files.activeTabId;files.newTab();secondId=files.activeTabId;phase=2;return;}
  if(phase===2){service.navigate(base+"/Second");phase=3;return;}
  if(phase===3){select();service.viewMode="list";files.newTab();thirdId=files.activeTabId;phase=4;return;}
  if(phase===4){service.navigate(base+"/Third");phase=5;return;}
  if(phase===5){
   select();thirdHistory=service.history.map(entry=>({id:entry.id,path:typeof entry==="string"?entry:entry.path}));thirdHistoryIndex=service.historyIndex;
   files.reorderTab(firstId,2);check(files.activeTabId===thirdId,"Reorder changed active identity");check(service.path===base+"/Third","Reorder navigated");
   files.closeTabId(firstId);check(files.activeTabId===thirdId && service.path===base+"/Third","Inactive close navigated");
   files.switchTab(files.tabs.findIndex(tab=>tab.id===secondId));phase=6;return;
  }
  if(phase===6){
   check(service.path===base+"/Second","Wrong selected tab location");check(files.selectedPaths[0]===base+"/Second/selected.txt","Tab selection not restored");check(service.viewMode==="list","Tab view not restored");
   files.closeTabId(secondId);phase=7;return;
  }
  if(phase===7){
   check(files.activeTabId===thirdId && service.path===base+"/Third","Active close chose wrong tab");check(JSON.stringify(service.history.map(entry=>({id:entry.id,path:typeof entry==="string"?entry:entry.path})))===JSON.stringify(thirdHistory) && service.historyIndex===thirdHistoryIndex,"Close modified destination history");check(files.selectedPaths[0]===base+"/Third/selected.txt","Close lost selected object");
   files.newTab();const temporaryId=files.activeTabId;files.switchTab(files.tabs.findIndex(tab=>tab.id===thirdId));files.closeTabId(temporaryId);phase=75;return;
  }
  if(phase===75){
   check(files.tabs.length===1 && files.activeTabId===thirdId,"Pending close changed tab identity");check(service.path===base+"/Third" && JSON.stringify(service.history.map(entry=>({id:entry.id,path:typeof entry==="string"?entry:entry.path})))===JSON.stringify(thirdHistory),"Rapid switch saved another tab state");check(files.selectedPaths[0]===base+"/Third/selected.txt","Rapid switch lost selection");
   files.collapseSidebar("favourites",true);files.collapseSidebar("locations",true);service.sidebarWidth=340;phase=8;return;
  }
  if(phase===8){
   check(files.groupCollapsed("favourites") && files.groupCollapsed("locations"),"Groups did not collapse");check(service.path===base+"/Third" && files.selectedPaths[0]===base+"/Third/selected.txt","Sidebar operation navigated/lost selection");
   phase=9;return;
  }
  if(phase===9){check(service.sidebarWidth===340,"Narrow clamp overwrote saved width");files.collapseSidebar("favourites",false);files.collapseSidebar("locations",false);check(files.selectedPaths.length===1,"Expand lost selection");console.log("FILES_LAYOUT_PASS");Qt.quit();}
 }catch(error){console.error("FILES_LAYOUT_FAIL "+error);Qt.quit();}}}
}
import QtQuick
import Quickshell
import "LUMEN_ASSETS" as D
ShellRoot {
 id:root
 property int phase:0
 property int ticks:0
 D.SearchState {id:search}
 QtObject {id:notifications;property bool dnd:false;property bool available:false}
 QtObject {id:audio;property bool muted:false;property bool available:false}
 QtObject {id:exported;property var state:({menu:null})}
 QtObject {id:stub;property var search:search;property var notifications:notifications;property var audioState:audio;property var exportedMenu:exported;property var entries:[];property date currentDate:new Date();function appLabel(e){return e.name;}}
 Item {width:1000;height:900;D.AppLauncher {id:launcher;shell:stub}}
 function find(item,predicate){if(predicate(item))return item;for(const child of item.children || []){const match=find(child,predicate);if(match)return match;}return null;}
 function press(text){const button=find(launcher,item=>item.text===text && typeof item.clicked==="function");if(!button)throw new Error("Button unavailable: "+text);button.clicked();}
 Timer {interval:50;repeat:true;running:true;onTriggered:{
  try {
   if(++root.ticks>120)throw new Error("Timed out phase "+root.phase);
   const state=root.find(launcher,item=>item.objectName==="clipboard-history-state");
   if(root.phase===0){launcher.setProvider("Clipboard");root.phase=1;}
   else if(root.phase===1 && state.available && !state.busy){if(state.enabled)throw new Error("Consent skipped");root.press("Enable Clipboard History");root.phase=2;}
   else if(root.phase===2 && state.enabled && !state.busy && launcher.results.length===1){state.search("copied");root.phase=3;}
   else if(root.phase===3 && !state.busy && launcher.results.length===1){launcher.launch(0,false);root.phase=4;}
   else if(root.phase===4 && !state.busy && state.message.includes("Paste")){if(!launcher.visible)throw new Error("Copy closed overlay");root.press("Clear History");root.phase=5;}
   else if(root.phase===5 && !state.busy && state.entries.length===0){root.press("Disable History");root.phase=6;}
   else if(root.phase===6 && !state.busy && !state.enabled){console.log("PASS clipboard launcher protocol");Qt.quit();}
  }catch(error){console.error("FAIL "+error);Qt.quit();}
 }}
}

import QtQuick
import Quickshell
import "assets/shell" as D
ShellRoot {
 id:test
 property int phase:0
 property int ticks:0
 property var savedRows:[]
 property var menu:null
 QtObject {id:exported;property var state:({appName:"",title:"",windowId:0})}
 QtObject {id:stub;property int focusEpoch:0;property var windows:[];property var exportedMenu:exported;function activeFiles(window){}function focusWindow(window){}}
 D.FilesState {id:service;persistViewPreferences:false;path:"/home/fixture/Documents"}
 D.FilesWindow {id:files;shell:stub;service:service;visible:true}
 function check(value,message){if(!value)throw Error(message+" phase="+phase);}
 function find(name,item,seen){if(!item || seen.includes(item))return null;seen.push(item);if(item.objectName===name)return item;const next=Array.from(item.children || []).concat(Array.from(item.resources || []));if(item.contentItem)next.push(item.contentItem);for(const child of next){const result=find(name,child,seen);if(result)return result;}return null;}
 function select(index){files.choose(index,0);}
 function open(){service.error="";menu.open();}
 function activate(){const row=find("files-action-nearby-send",files.contentItem,[]);check(row && row.enabled,"actual sharing row must be enabled");row.clicked();}
 Timer {interval:50;running:true;repeat:true;onTriggered:{try{
  if(++test.ticks>120)throw Error("Fixture readiness deadline");
  if(test.phase===0){if(!service.ready || service.busy || service.rows.length!==2)return;test.menu=find("files-actions-menu",files.contentItem,[]);check(test.menu,"actual menu found");test.savedRows=service.rows;D.NearbySharingState.state={available:true,active:false,canSend:true,detail:"Off"};D.NearbySharingState.observedAt=Date.now();D.NearbySharingState.now=Date.now();select(0);open();}
  else if(test.phase===1){activate();check(D.AppLaunch.recordedCommands.length===1,"unchanged selection dispatched exactly once");check(D.AppLaunch.recordedCommands[0][2]==="/home/fixture/Documents/a $(literal).txt","literal selected argument");select(0);open();select(1);}
  else if(test.phase===2){activate();check(D.AppLaunch.recordedCommands.length===1 && service.error.includes("changed"),"later selection must not dispatch");select(0);open();const rows=service.rows.slice();rows[0]=Object.assign({},rows[0],{identity:{device:rows[0].identity.device,inode:rows[0].identity.inode+1}});service.rows=rows;}
  else if(test.phase===3){activate();check(D.AppLaunch.recordedCommands.length===1 && service.error.includes("changed"),"replaced row must not dispatch");service.rows=test.savedRows;select(1);open();}
  else if(test.phase===4){activate();check(D.AppLaunch.recordedCommands.length===2 && D.AppLaunch.recordedCommands[1][2]==="/home/fixture/Documents/b.txt","fresh explicit menu dispatch");console.log("FILES_SHARE_MENU_PASS "+JSON.stringify({commands:D.AppLaunch.recordedCommands,checks:4}));Qt.quit();}
  ++test.phase;
 }catch(error){console.error("FILES_SHARE_MENU_FAIL "+error);Qt.exit(1);}}}
 Component.onCompleted:service.navigate("/home/fixture/Documents")
}

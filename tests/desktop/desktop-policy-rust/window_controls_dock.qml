import QtQuick
import Quickshell
import Quickshell.Io
import Quickshell.Wayland
import "assets/shell" as D
ShellRoot {id:stub
 property var referenceDock:[{key:"documents",canvas:68,entry:{id:"synthetic-documents",name:"Documents",icon:"application-x-executable"}}]
 property string popupOwner:""
 property bool dockPreferencesBusy:false
 property string aliasError:""
 property string applicationCommandError:""
 property date currentDate:new Date()
 property var allWindows:ToplevelManager.toplevels.values
 property var focuses:[]
 signal requestDockFocus(string name)
 signal dismissPopups(string owner)
 function claimPopup(owner){popupOwner=owner;dismissPopups(owner);}
 function windowsFor(entry){return allWindows.filter(w=>["Document One","Document Two"].includes(w.title)).sort((a,b)=>a.title.localeCompare(b.title));}
 function dockWindows(item){return windowsFor(item.entry);}
 function dockLabel(item){return item.entry.name;}
 function runningFor(entry){return windowsFor(entry).length>0;}
 function canConfigureDockEntry(entry){return false;}
 function isDockPinned(entry){return false;}
 function canForceQuit(entry){return false;}
 function focusWindow(window){focuses=focuses.concat([window.title]);window.activate();}
 function activate(entry,fresh){throw new Error("Unexpected application launch");}
 function activateDock(item,fresh,output){throw new Error("Unexpected application activation");}
 function updateDockPin(entry,action){}
 function reorderDockPin(a,b){}
 D.Dock {id:dock;screen:Quickshell.screens[0];shell:stub}
 IpcHandler {target:"windowContracts"
 function status():string{return JSON.stringify({open:dock.contextEntry!==null,index:dock.contextIndex,selected:dock.contextSelection?dock.contextSelection.row.label:"",windows:stub.windowsFor(stub.referenceDock[0].entry).map(w=>({title:w.title,minimized:w.minimized})),focuses:stub.focuses});}
 function open():void{dock.openContext(stub.referenceDock[0],0);}
 }
}

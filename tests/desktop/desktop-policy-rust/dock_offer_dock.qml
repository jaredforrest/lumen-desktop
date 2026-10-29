import QtQuick
import Quickshell
import Quickshell.Io
import "assets/shell" as D
ShellRoot {QtObject {id:stub
 property string popupOwner:""
 property date currentDate:new Date()
 property bool dockPreferencesBusy:false
 property string aliasError:""
 property string applicationCommandError:""
 property var referenceDock:[{key:"accept",entry:{id:"fixture-accept",name:"Offer Observer",icon:"application-x-executable"},canvas:68},{key:"refuse",entry:{id:"fixture-refuse",name:"Unsupported Observer",icon:"application-x-executable"},canvas:68}]
 signal requestDockFocus(string name)
 signal dismissPopups(string owner)
 function claimPopup(owner){popupOwner=owner;dismissPopups(owner);}
 function dockWindows(item){return [];}
 function dockLabel(item){return item.key;}
 function runningFor(entry){return false;}
 function activateDock(){throw Error("Unexpected activation");}
 function canConfigureDockEntry(){return false;}
 function isDockPinned(){return false;}
}
 D.Dock {id:dock;shell:stub;screen:Quickshell.screens[0]}
 IpcHandler {target:"dockOffer";function status():string{return JSON.stringify({keys:stub.referenceDock.map(row=>row.key),width:dock.width,slot:dock.slot,shelf:dock.shelfThickness,events:dock.fixtureEvents,command:dock.fixtureDropWorker.command,result:dock.fixtureResult,error:dock.dropError,busy:dock.fixtureDropWorker.running,drops:dock.fixtureDrops()});}}
}

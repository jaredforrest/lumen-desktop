import QtQuick
import Quickshell
import Quickshell.Io
import "."
ShellRoot {id:mock
 property var windows:[{appId:"a",title:"A"},{appId:"b",title:"B"},{appId:"a",title:"A2"}]
 property var activationHistory:windows
 property string popupOwner:""
 property int focusEpoch:0
 property int activated:0
 property string activatedApp:""
 signal dismissPopups(string owner)
 function entryFor(w){return null;}
 function nameForAppId(a,b){return a;}
 function preferredOutput(){return "";}
 function claimPopup(owner){++focusEpoch;popupOwner=owner;dismissPopups(owner);}
 function focusWindow(w){activated++;activatedApp=w.appId;Quickshell.execDetached(["swaymsg",'[app_id="'+w.appId+'"] focus']);}
 AppSwitcher {id:picker;shell:mock}
 IpcHandler {target:"fixture"
 function step(action:string):string {picker.command(action);return picker.status();}
 function raceEpoch():void {picker.command("next");picker.command("commit");++mock.focusEpoch;}
 function raceRemovedApp():void {picker.command("next");picker.command("commit");mock.windows=mock.windows.filter(w=>w.appId!=="b");}
 function status():string {return JSON.stringify({picker:JSON.parse(picker.status()),activated:mock.activated,app:mock.activatedApp});}
 }
}

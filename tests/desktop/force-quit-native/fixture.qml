import QtQuick
import Quickshell
import Quickshell.Io
import "@SHELL_URI@" as Desktop
ShellRoot {id:mock
 property string popupOwner:""
 signal dismissPopups(string owner)
 function preferredOutput(){return "";}
 function claimPopup(owner){popupOwner=owner;dismissPopups(owner);}
 Desktop.ForceQuitDialog {id:dialog;shell:mock}
 IpcHandler {target:"fixture"
 function openDialog(dark:bool):void {Desktop.Theme.colors=dark?({background:"#242426",field:"#343438",text:"#eeeeee",muted:"#aaaaaa",hover:"#454549",accent:"#0a84ff"}):({background:"#f2f2f4",field:"#ffffff",text:"#202023",muted:"#6b6b70",hover:"#dedee3",accent:"#007aff"});dialog.show("Disposable Fixture",[{appId:"fixture",title:"Fixture"}]);}
 function status():string {return JSON.stringify({phase:dialog.phase,visible:dialog.visible});}
 }
}

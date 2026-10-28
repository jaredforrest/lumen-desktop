import QtQuick
import Quickshell
import Quickshell.Io
ShellRoot {
 QtObject {id:stub
  property var referenceDock: Array.from({length:8},(_,i)=>({key:"item"+i,image:"files.svg",canvas:68,entry:null}))
  property bool dockPreferencesBusy:false
  property string popupOwner:""
  property date currentDate:new Date()
  signal requestDockFocus(string name)
  signal dismissPopups(string owner)
  function claimPopup(owner){popupOwner=owner;dismissPopups(owner);}
  function dockWindows(item){return [];}
  function dockLabel(item){return item.key;}
  function activateDock(item,shift,name){}
 }
 Dock {id:dock;shell:stub;screen:Quickshell.screens[0]}
 IpcHandler {target:"fixture"
  function state():string{return JSON.stringify({hovered:dock.hovered,slot:dock.pointerSlot,width:dock.width});}
 }
}
import QtQuick
import Quickshell
import Quickshell.Io
import Quickshell.Wayland
import "assets/shell" as D
import "assets/shell/DockModel.js" as Model
ShellRoot {id:stub
 property var entry:({id:"synthetic-documents",name:"Documents",icon:"application-x-executable"})
 property var referenceDock:Model.build([], [entry],[entry],[],[],false,[],[],mini.items)
 property string popupOwner:""
 property bool dockPreferencesBusy:false
 property string aliasError:""
 property string applicationCommandError:""
 property date currentDate:new Date()
 property var windows:ToplevelManager.toplevels.values.filter(w=>w.title==="Same document title")
 property var firstWindow:null
 property var captured:null
 property int attempts:0
 property int launches:0
 signal requestDockFocus(string name)
 signal dismissPopups(string owner)
 function claimPopup(owner){popupOwner=owner;dismissPopups(owner);}
 function windowsFor(e){return windows;}
 function dockWindows(item){return item.action==="minimized-window"?(mini.valid(item)?[item.window]:[]):windows;}
 function dockLabel(item){return item.action==="minimized-window"?item.name:entry.name;}
 function runningFor(e){return windows.length>0;}
 function canConfigureDockEntry(e){return false;}
 function isDockPinned(e){return false;}
 function canForceQuit(e){return false;}
 function activateDock(item,fresh,output){if(item.action!=="minimized-window"){launches++;return;}attempts++;mini.activate(item);}
 function activate(e,fresh){launches++;}
 function focusWindow(window){window.activate();}
 function updateDockPin(e,a){}
 function reorderDockPin(a,b){}
 D.MinimizedWindowState {id:mini;enabled:D.Theme.preferences.dockMinimizeDestination==="windows";windows:stub.windows;entryForWindow:w=>stub.entry}
 D.Dock {id:dock;screen:Quickshell.screens.find(s=>s.name==="HEADLESS-1");shell:stub}
 IpcHandler {target:"minimizedDock"
 function status():string{return JSON.stringify({output:dock.outputName,mode:D.Theme.preferences.dockMinimizeDestination,keyboard:dock.keyboardMode,keyboardIndex:dock.keyboardIndex,active:dock.contentItem.Window.window?dock.contentItem.Window.window.active:false,width:dock.width,shelf:dock.shelfThickness,slot:dock.slot,hovered:dock.hovered,pressed:dock.pressedKey,keys:stub.referenceDock.map(i=>i.key),entries:mini.items.map(i=>({key:i.key,name:i.name})),records:mini.records.map(r=>r.key),minimized:stub.windows.map(w=>w.minimized),attempts:stub.attempts,launches:stub.launches});}
 function mode(value:string):void{D.Theme.preferences=Object.assign({},D.Theme.preferences,{dockMinimizeDestination:value});}
 function focus():void{stub.requestDockFocus("HEADLESS-1");}
 function observeFirst():void{stub.firstWindow=stub.windows[0];}
 function firstKey():string{const item=mini.items.find(i=>i.window===stub.firstWindow);return item?item.key:"";}
 function capture(key:string):void{stub.captured=mini.items.find(i=>i.key===key);}
 function stale():bool{return mini.activate(stub.captured);}
 function capturedKey():string{const item=mini.items.find(i=>stub.captured && i.window===stub.captured.window);return item?item.key:"";}
 function cycleMode():void{D.Theme.preferences=Object.assign({},D.Theme.preferences,{dockMinimizeDestination:"application"});D.Theme.preferences=Object.assign({},D.Theme.preferences,{dockMinimizeDestination:"windows"});}
 }
}

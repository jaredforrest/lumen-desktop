import QtQuick
import Quickshell
import Quickshell.Io
import "DockModel.js" as Model
ShellRoot {
 SettingsState {id:settings;active:true}
 QtObject {id:stub
  property var apps:[{id:"fixture-a",name:"Fixture A",icon:"application-x-executable"},{id:"fixture-b",name:"Fixture B",icon:"application-x-executable"}]
  property var reference:[{key:"files",action:"files",image:"files.svg",canvas:68},{key:"a",appId:"fixture-a",canvas:68},{key:"b",appId:"fixture-b",canvas:68},{key:"downloads",action:"downloads",image:"files.svg",canvas:68},{key:"trash",action:"trash",image:"files.svg",canvas:68}]
  property bool forcedBusy:false
  property bool runningA:false
  property bool replaced:false
  property bool dockPreferencesBusy:settings.busy || forcedBusy
  property var referenceDock:Model.build(reference,apps,runningA?[apps[0]]:[],settings.preferences.dockPins,[],false,settings.preferences.dockFolders,settings.preferences.dockFiles)
  property string popupOwner:""
  property date currentDate:new Date()
  property var activations:[]
  property string aliasError:""
  property string applicationCommandError:""
  signal requestDockFocus(string name)
  signal dismissPopups(string owner)
  function claimPopup(owner){popupOwner=owner;dismissPopups(owner);}
  function dockWindows(item){return [];}
  function dockLabel(item){return item.key;}
  function runningFor(entry){return runningA && entry.id==="fixture-a";}
  function activateDock(item,shift,name){activations=activations.concat(item.key);}
  function canConfigureDockEntry(entry){return entry.id!=="lumen-files";}
  function isDockPinned(entry){return (settings.preferences.dockPins || []).includes(entry.id);}
  function updateDockPin(entry,action){if(!dockPreferencesBusy)settings.setPreference("dockPins",Model.updatePins(settings.preferences.dockPins,entry.id,action));}
  function reorderDockPin(source,target){if(!dockPreferencesBusy)settings.setPreference("dockPins",Model.movePin(settings.preferences.dockPins,source.id,target.id));}
  function removeDockAlias(item){if(dockPreferencesBusy)return;if(item.action==="file")settings.setPreference("dockFiles",settings.preferences.dockFiles.filter(path=>path!==item.path));else if(item.action==="folder")settings.setPreference("dockFolders",settings.preferences.dockFolders.filter(folder=>folder.path!==item.folder.path));}
 }
 Dock {id:dock;shell:stub;screen:Quickshell.screens[0]}
 IpcHandler {target:"fixture"
  function state():string{return JSON.stringify({keys:stub.referenceDock.map(x=>x.key),width:dock.width,slot:dock.slot,dragging:dock.dragging,hovered:dock.hovered,activations:stub.activations,busy:stub.dockPreferencesBusy,ready:settings.available,pins:settings.preferences.dockPins,files:settings.preferences.dockFiles,folders:settings.preferences.dockFolders});}
  function busy(value:bool):void{stub.forcedBusy=value;}
  function running():void{stub.runningA=true;}
  function replace():void{stub.apps=stub.apps.map(x=>Object.assign({},x));}
  function remove():void{stub.apps=stub.apps.filter(x=>x.id!=="fixture-a");}
  function reset():void{stub.forcedBusy=false;stub.runningA=false;stub.apps=[{id:"fixture-a",name:"Fixture A",icon:"application-x-executable"},{id:"fixture-b",name:"Fixture B",icon:"application-x-executable"}];settings.refresh();}
 }
}
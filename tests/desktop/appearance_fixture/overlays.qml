import QtQuick
import QtQuick.Controls
import Quickshell
import Quickshell.Wayland
import Quickshell.Io
import "assets/shell" as D
ShellRoot {
 id:audit
 property string family:""
 property var actions:[]
 QtObject {id:searchService
  property var files:[];property var preview:({});property bool opening:false;property bool searching:false
  property var resourceRetirement:null
  signal opened()
  signal browseRequested(string path)
  function search(query){}function requestPreview(path,token){}function refresh(){}
 }
 QtObject {id:notifications
  property string error:"";property bool available:true;property bool dnd:false;property var policies:({});property var expandedCards:({})
  property var history:[({id:7,app:"Private application",desktop:"",summary:"Retained notification",body:"Synthetic visible body",icon:"",image:"",timestamp:Date.now()+3600000,actions:[{id:"default",label:"Open"}],urgency:1,resident:true,transient:false,banner:false,unread:true})]
  property var groups:[({key:"private",rows:history})]
  function command(value){audit.actions=audit.actions.concat(JSON.stringify(value));}function invoke(row,key){audit.actions=audit.actions.concat(key);}
 }
 QtObject {id:exported;property var state:({menu:null,token:""});function label(value){return value;}}
 QtObject {id:stub
  property string popupOwner:"";property int focusEpoch:0;property var activationHistory:[];property var applicationActivationHistory:[];property var runningApplications:[]
  property var windows:[({appId:"fixture.first",title:"First document",id:101,pid:11}),({appId:"fixture.second",title:"Second document",id:102,pid:12})]
  property var entries:[({id:"fixture.first",name:"First application",icon:"application-x-executable",noDisplay:false}),({id:"fixture.second",name:"Second application",icon:"application-x-executable",noDisplay:false})]
  property var groupMembershipDrag:null;property date currentDate:new Date(2026,9,20)
  property alias search:searchService;property alias notifications:notifications;property alias exportedMenu:exported
  property string volume:""
  property var audioState:({available:false,muted:false});property var systemState:({wifi:{available:false,networks:[]},power:{available:false,percentage:0},internet:{available:false}})
  property var vpn:({mullvad:{installed:false,state:"unavailable"},tailscale:{installed:false,state:"unavailable"}})
  signal dismissPopups(string owner)
  function claimPopup(owner){popupOwner=owner;dismissPopups(owner);}
  function entryFor(window){return entries.find(item=>item.id===window.appId) || null;}
  function appLabel(entry){return entry.name;}function appIcon(entry){return Qt.resolvedUrl("assets/shell/icons/application.svg");}
  function nameForAppId(id,title){return title;}function preferredOutput(){return "HEADLESS-1";}
  function runningFor(entry){return true;}function applicationInstanceKey(entry){return entry.id;}
  function activate(entry){audit.actions=audit.actions.concat(entry.id);}function focusWindow(window){audit.actions=audit.actions.concat(window.id);}
  function iconPath(icon){return "";}function openSettings(page){audit.actions=audit.actions.concat("settings");}
 }
 PanelWindow {id:layer;visible:["launcher","controls","notifications"].includes(audit.family);anchors {top:true;left:true;right:true;bottom:true}exclusiveZone:0;exclusionMode:ExclusionMode.Ignore;color:"transparent";WlrLayershell.layer:WlrLayer.Overlay;WlrLayershell.namespace:"lumen-live-appearance-fixture";WlrLayershell.keyboardFocus:visible?WlrKeyboardFocus.Exclusive:WlrKeyboardFocus.None
  D.AppLauncher {id:launcher;visible:audit.family==="launcher";shell:stub;anchors.centerIn:parent;onCloseRequested:layer.visible=false}
  D.PanelSurface {id:centreSurface;visible:audit.family==="controls";x:400;y:80;width:340;height:centre.height+32
   D.ControlCentre {id:centre;x:16;y:16;width:parent.width-32;shell:stub;onSettingsRequested:audit.actions=audit.actions.concat("settings")}
  }
  D.PanelSurface {id:notificationSurface;visible:audit.family==="notifications";x:400;y:120;width:360;height:notificationCentre.height+32
   D.NotificationCentre {id:notificationCentre;x:16;y:16;width:parent.width-32;service:notifications}
  }
 }
 D.AppSwitcher {id:switcher;shell:stub}
 D.WindowOverview {id:overview;shell:stub}
 function walk(item){let rows=[item];for(const child of item.children || [])rows=rows.concat(walk(child));return rows;}
 function owner(){return family==="switcher"?switcher:family==="overview"?overview:layer;}
 function body(){return family==="launcher"?launcher:family==="controls"?centreSurface:family==="notifications"?notificationSurface:owner().contentItem;}
 function shown(item,body){for(let parent=item;parent;parent=parent.parent){if(!parent.visible)return false;if(parent===body)return true;}return false;}
 function box(item,body){const at=item.mapToItem(body,0,0);return {x:at.x,y:at.y,width:item.width,height:item.height};}
 function target(){const item=body();return walk(item).find(child=>family==="controls"?child.text==="System Settings…":family==="notifications"?child.objectName==="notification-card":family==="overview"?child.placeholderText==="Find a window":child.Accessible && child.Accessible.name==="Search applications, files, actions and clipboard history");}
 function state(){const window=owner(),item=body(),origin=item.mapToItem(window.contentItem,0,0);return {family:family,dark:D.Theme.dark,reduced:!!D.Theme.preferences.reduceTransparency,visible:window.visible,width:window.width,height:window.height,body:{x:origin.x,y:origin.y,width:item.width,height:item.height},selected:switcher.selected,targetFocus:target()?target().activeFocus:false,query:family==="launcher"?launcher.queryText:family==="overview" && target()?target().text:"",launcher:launcher.status(),actions:actions,texts:walk(item).filter(child=>shown(child,item) && child.text!==undefined && child.font!==undefined && child.color!==undefined && String(child.text)).map(child=>({text:String(child.text),color:child.color.toString(),width:child.width,height:child.height,rect:box(child,item)})),rectangles:walk(item).filter(child=>shown(child,item) && child.radius!==undefined && child.color!==undefined && child.width>20 && child.height>15).map(child=>({width:child.width,height:child.height,color:child.color.toString(),alpha:child.color.a})),focus:walk(window.contentItem).filter(child=>child.activeFocus).map(child=>child.Accessible?child.Accessible.name:""),overview:{selected:overview.selected,count:overview.matches.length}};}
 IpcHandler {target:"liveOverlay"
  function open(value:string):void {audit.actions=[];layer.visible=false;switcher.visible=false;overview.visible=false;audit.family=value;if(value==="switcher")switcher.command("begin");else if(value==="overview")overview.show("");else {layer.visible=true;if(value==="launcher"){launcher.setProvider("Applications");Qt.callLater(launcher.focusSearch);}}}
  function focusTarget():bool{const item=audit.target();if(!item)return false;item.forceActiveFocus();return item.activeFocus;}
  function status():string{return JSON.stringify(audit.state());}
 }
}

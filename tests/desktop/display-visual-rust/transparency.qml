import QtQuick
import QtQuick.Controls
import QtQuick.Window
import Quickshell
import Quickshell.Wayland
import Quickshell.Io
import "assets/shell" as D
ShellRoot {
 id:root
 property string active:""
 property var records:[]
 function record(value){records=records.concat(value);}
 QtObject {id:stub
  property string popupOwner:""
  property date currentDate:new Date(2026,9,20)
  property bool dockPreferencesBusy:false
  property string aliasError:""
  property string applicationCommandError:""
  property var referenceDock:[{key:"one",action:"fixture",image:"files.svg",canvas:68},{key:"two",action:"fixture",image:"files.svg",canvas:68},{key:"three",action:"fixture",image:"calendar.svg",canvas:68}]
  signal requestDockFocus(string name)
  signal dismissPopups(string owner)
  function claimPopup(owner){popupOwner=owner;dismissPopups(owner);}
  function preferredOutput(){return "HEADLESS-1";}
  function dockWindows(item){return [];}
  function dockLabel(item){return item.key;}
  function runningFor(entry){return false;}
  function activateDock(item,shift,name){root.record("dock:"+item.key);}
  function canConfigureDockEntry(entry){return false;}
  function isDockPinned(entry){return false;}
 }
 QtObject {id:notifications
  property bool available:true
  property var policies:({})
  property var expandedCards:({})
  function invoke(row,key){root.record("notification:"+row.id+":"+key);}
  function command(value){}
 }
 PanelWindow {id:backdrop;anchors {left:true;right:true;top:true;bottom:true}exclusionMode:ExclusionMode.Ignore;exclusiveZone:0;color:"transparent";WlrLayershell.layer:WlrLayer.Background;WlrLayershell.namespace:"lumen-material-test-background"
  Repeater {model:Math.ceil(backdrop.width/32)*Math.ceil(backdrop.height/32)
   Rectangle {required property int index;x:(index%Math.ceil(backdrop.width/32))*32;y:Math.floor(index/Math.ceil(backdrop.width/32))*32;width:32;height:32;color:((Math.floor(x/32)+Math.floor(y/32))%2)?"#e020a0":"#10d0e0"}
  }
 }
 D.Dock {id:dock;shell:stub;screen:Quickshell.screens[0]}
 D.RemoteDesktopWindow {id:remote;shell:stub}
 D.HelpWindow {id:help;shell:stub}
 D.SessionOptions {id:session;executeActions:false;onConfirmed:action=>root.record("session:"+action)}
 PanelWindow {id:popover;visible:root.active==="panel";anchors {top:true;right:true}margins {top:80;right:50}implicitWidth:320;implicitHeight:140;exclusionMode:ExclusionMode.Ignore;color:"transparent";WlrLayershell.layer:WlrLayer.Overlay;WlrLayershell.keyboardFocus:WlrKeyboardFocus.OnDemand
  D.PanelSurface {id:panel;anchors.fill:parent
   Column {x:18;y:24;width:parent.width-36;spacing:12
    D.PanelLabel {text:"Material policy";font.weight:Font.DemiBold}
    D.ControlRow {id:panelAction;width:parent.width;text:"Run private panel action";onClicked:root.record("panel")}
   }
  }
 }
 PanelWindow {id:calendarPopup;visible:root.active==="calendar";anchors {top:true;right:true}margins {top:80;right:50}implicitWidth:344;implicitHeight:calendar.height;exclusionMode:ExclusionMode.Ignore;color:"transparent";WlrLayershell.layer:WlrLayer.Overlay;WlrLayershell.keyboardFocus:WlrKeyboardFocus.OnDemand
  D.CalendarCard {id:calendar;width:parent.width;today:new Date(2026,9,20);compact:true;active:false;onOpenCalendar:day=>root.record("calendar:"+Qt.formatDateTime(day,"yyyy-MM-dd"))}
 }
 PanelWindow {id:banner;visible:root.active==="notification";anchors {top:true;right:true}margins {top:80;right:50}implicitWidth:350;implicitHeight:card.height;exclusionMode:ExclusionMode.Ignore;color:"transparent";WlrLayershell.layer:WlrLayer.Overlay;WlrLayershell.keyboardFocus:WlrKeyboardFocus.OnDemand
  D.NotificationCard {id:card;width:parent.width;service:notifications;notification:({id:7,app:"Private fixture",desktop:"",summary:"Stable notification title",body:"An owned notification remains readable.",icon:"",image:"",timestamp:Date.now()+3600000,actions:[{id:"default",label:"Open"}],urgency:1,resident:true,transient:false,banner:true,unread:true})}
 }
 function find(item,name){if(item.objectName===name || (item.Accessible && item.Accessible.name===name))return item;for(const child of item.children || []){const found=find(child,name);if(found)return found;}if(item.contentItem && item.contentItem!==item){const found=find(item.contentItem,name);if(found)return found;}return null;}
 function owner(){return ({remote:remote,help:help,session:session,panel:popover,calendar:calendarPopup,notification:banner,dock:dock})[active];}
 function selectedTarget(){const window=owner();return active==="remote"?find(remote.contentItem,"VNC"):active==="help"?find(help.contentItem,"Search Help"):active==="session"?find(session.contentItem,"Cancel"):active==="panel"?panelAction:active==="calendar"?find(calendar,"calendar-open-related"):active==="notification"?card:null;}
 function rect(item,reference){const at=item.mapToItem(reference || root.owner().contentItem,0,0);return {x:at.x,y:at.y,width:item.width,height:item.height};}
 function dockMaterial(item){if(!item.visible)return null;if(item.radius===22 && item.width===dock.width && item.height===dock.shelfThickness && item.color && item.color.a>0)return item.color;for(const child of item.children || []){const color=dockMaterial(child);if(color)return color;}return null;}
 function material(){if(active==="panel")return panel.color;if(active==="calendar")return calendar.color;if(active==="notification")return card.color;if(active==="remote")return D.Theme.sidebarMaterial;if(active==="dock")return dockMaterial(dock.contentItem) || Qt.color(D.Theme.colors.background);return Qt.color(D.Theme.colors.background);}
 IpcHandler {target:"transparencyAudit"
  function open(name:string):void {remote.visible=false;help.visible=false;session.visible=false;root.active=name;root.records=[];if(name==="remote"){remote.filter="All";remote.visible=true;}else if(name==="help"){find(help.contentItem,"Search Help").text="";help.show();}else if(name==="session")session.show("suspend");}
  function focusDock():void{stub.requestDockFocus("HEADLESS-1");}
  function focusTarget():bool {const item=root.selectedTarget();if(!item)return false;item.forceActiveFocus();return item.activeFocus;}
  function state():string {
   const window=root.owner(),item=root.selectedTarget();let actions=[],texts=[];
   function scan(item){if(!item.visible || item.width<=0 || item.height<=0)return;
    if(item.Accessible && item.Accessible.name && item.enabled && (typeof item.clicked==="function" || item.activeFocusOnTab))actions.push({name:item.Accessible.name,rect:rect(item)});
    if(item.text!==undefined && item.font!==undefined && String(item.text))texts.push({text:String(item.text),rect:rect(item),size:item.font.pixelSize,truncated:!!item.truncated});
    for(const child of item.children || [])scan(child);
   }
   if(window)scan(window.contentItem);
   const color=root.material();
   return JSON.stringify({active:active,helpQuery:help.query,reduced:!!D.Theme.preferences.reduceTransparency,dark:D.Theme.dark,window:window?rect(window.contentItem):null,layerMargins:window && window.margins!==undefined?{top:window.margins.top,right:window.margins.right}:null,target:item?rect(item):null,focused:item?item.activeFocus:false,actions:actions,texts:texts,records:root.records,filter:remote.filter,visible:window?window.visible:false,material:{r:color.r,g:color.g,b:color.b,a:color.a},dock:{rect:rect(dock.contentItem,dock.contentItem),width:dock.width,height:dock.height,marginBottom:dock.margins.bottom,slot:dock.slot,hovered:dock.hovered,keyboard:dock.keyboardMode,keyboardIndex:dock.keyboardIndex,mask:{x:dock.mask.x,y:dock.mask.y,width:dock.mask.width,height:dock.mask.height},zone:dock.exclusiveZone,keys:stub.referenceDock.map(item=>item.key)}});
  }
 }
}
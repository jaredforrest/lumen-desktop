import QtQuick
import QtQuick.Controls
import QtQuick.Window
import Quickshell
import Quickshell.Io
import "assets/shell" as D
ShellRoot {
 id:audit
 property var actions:[]
 property int confirmations:0
 property bool tracing:false
 property bool traceOverflow:false
 property var trace:[]
 QtObject {id:stub
  property string popupOwner:""
  property var bars:({})
  property date currentDate:new Date(2026,9,20)
  property bool dockPreferencesBusy:false
  property string aliasError:""
  property string applicationCommandError:""
  property var referenceDock:[{key:"help",action:"fixture",image:"files.svg",canvas:68},{key:"other",action:"fixture",image:"calendar.svg",canvas:68}]
  property alias notifications:notifications
  property string hudOutput:"HEADLESS-1"
  property bool hudVisible:false
  property var audioState:({available:true,muted:false,level:0.5})
  signal requestDockFocus(string name)
  signal dismissPopups(string owner)
  function claimPopup(owner){popupOwner=owner;dismissPopups(owner);}
  function preferredOutput(){return "HEADLESS-1";}
  function dockWindows(item){return [];}
  function dockLabel(item){return item.key;}
  function runningFor(entry){return false;}
  function activateDock(item,shift,name){audit.actions=audit.actions.concat(item.key);if(item.key==="help")help.show();}
  function canConfigureDockEntry(entry){return false;}
  function isDockPinned(entry){return false;}
  function showMenu(name,output){}
 }
 D.HelpWindow {id:help;shell:stub}
 D.SessionOptions {id:session;executeActions:false;onConfirmed:audit.confirmations++}
 D.NotificationState {id:notifications;shell:stub}
 D.NotificationBanners {id:banners;shell:stub;outputName:"HEADLESS-1";outputWidth:1100;outputHeight:800}
 D.Dock {id:dock;shell:stub}
 D.VolumeHud {id:hud;shell:stub;outputName:"HEADLESS-1"}
 function walk(item,seen){if(!item || seen.includes(item))return [];seen.push(item);let found=[item];for(const child of Array.from(item.children || []).concat(Array.from(item.resources || [])))found=found.concat(walk(child,seen));for(const key of ["contentItem","animation"])if(item[key])found=found.concat(walk(item[key],seen));return found;}
 function rect(item,owner){const point=item.mapToItem(owner.contentItem,0,0);return {x:point.x,y:point.y,width:item.width,height:item.height};}
 function icons(){return walk(dock.contentItem,[]).filter(item=>item.iconSource!==undefined && item.parent && item.parent.magnification!==undefined);}
 Timer {interval:4;repeat:true;running:audit.tracing;onTriggered:{if(audit.trace.length>=1024){audit.traceOverflow=true;audit.tracing=false;return;}const card=audit.walk(banners.contentItem,[]).find(item=>item.objectName==="notification-card");audit.trace=audit.trace.concat({widths:audit.icons().map(item=>item.width),banner:card?card.opacity:null});}}

 IpcHandler {target:"motionAudit"
  function beginTrace():void {audit.trace=[];audit.traceOverflow=false;audit.tracing=true;}
  function endTrace():string {audit.tracing=false;return JSON.stringify({overflow:audit.traceOverflow,samples:audit.trace});}
  function focusDock():void {stub.requestDockFocus("HEADLESS-1");}
  function showSession(action:string):void {session.show(action);}
  function showHud(value:bool):void {stub.hudVisible=value;}
  function state():string {
   const items=walk(banners.contentItem,[]),card=items.find(item=>item.objectName==="notification-card"),summary=card?walk(card,[]).find(item=>item.text===card.notification.summary && item.font):null,dismiss=card?walk(card,[]).find(item=>item.objectName==="notification-dismiss"):null;
   const icons=audit.icons();
   const surface=walk(dock.contentItem,[]).find(item=>item!==dock.contentItem && String(item).startsWith("QQuickItem") && item.width===dock.width && item.height===dock.height && item.opacity!==undefined),hudSurface=walk(hud.contentItem,[]).find(item=>item.radius===22 && item.opacity!==undefined),search=walk(help.contentItem,[]).find(item=>item.placeholderText==="Search Help");
   return JSON.stringify({reduced:!!D.Theme.preferences.reduceMotion,help:{visible:help.visible,active:!!help.contentItem.Window.window && help.contentItem.Window.window.active,query:help.query,search:!!search && search.activeFocus,width:help.width,height:help.height},session:{visible:session.visible,active:!!session.contentItem.Window.window && session.contentItem.Window.window.active,focus:session.contentItem.Window.window && session.contentItem.Window.window.activeFocusItem?session.contentItem.Window.window.activeFocusItem.Accessible.name:"",confirmations:audit.confirmations},dock:{active:!!dock.contentItem.Window.window && dock.contentItem.Window.window.active,keyboard:dock.keyboardMode,index:dock.keyboardIndex,hovered:dock.hovered,revealed:dock.revealed,surfaceOpacity:surface?surface.opacity:null,icons:icons.map(item=>({key:item.parent.modelData.key,width:item.width,target:item.parent.modelData.canvas*dock.artworkScale+18*item.parent.magnification}))},hud:{visible:hud.visible,opacity:hudSurface?hudSurface.opacity:null,width:hud.width,height:hud.height,bottom:hud.margins.bottom},notifications:{available:notifications.available,rows:notifications.rows.map(row=>({id:row.id,summary:row.summary,body:row.body,banner:row.banner})),banners:notifications.banners.length,visible:banners.visible,opacity:card?card.opacity:null,width:banners.width,height:banners.height,right:banners.margins.right,top:banners.margins.top,summary:summary?{rect:rect(summary,banners),color:String(summary.color)}:null,dismiss:dismiss?{visible:dismiss.visible,rect:rect(dismiss,banners)}:null},actions:audit.actions});
  }
 }
}

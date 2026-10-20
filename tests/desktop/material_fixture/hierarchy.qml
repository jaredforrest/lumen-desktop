import QtQuick
import QtQuick.Controls
import Quickshell
import Quickshell.Wayland
import Quickshell.Io
import "assets/shell" as D
ShellRoot {
 id:root
 property string phase:"base"
 Component.onCompleted:Qt.callLater(()=>service.navigate(service.path,0))
 QtObject {id:exported;property var state:({appName:"",title:"",windowId:0})}
 QtObject {id:stub
  property int focusEpoch:0;property var windows:[];property var exportedMenu:exported
  property date currentDate:new Date(2026,9,20)
  property string hudOutput:"HEADLESS-1";property bool hudVisible:root.phase==="hud"
  property var audioState:({available:true,muted:false,level:0.55})
  function activeFiles(window){}function focusWindow(window){}function claimPopup(name){}function focusOwnedWindow(name){}function openSettings(page){}
 }
 QtObject {id:notifications;property bool available:true;property var policies:({});property var expandedCards:({});function invoke(row,key){}function command(value){}}
 PanelWindow {id:backdrop;anchors {left:true;right:true;top:true;bottom:true}exclusionMode:ExclusionMode.Ignore;exclusiveZone:0;color:"transparent";WlrLayershell.layer:WlrLayer.Background
  Repeater {model:Math.ceil(backdrop.width/32)*Math.ceil(backdrop.height/32)
   Rectangle {required property int index;x:(index%Math.ceil(backdrop.width/32))*32;y:Math.floor(index/Math.ceil(backdrop.width/32))*32;width:32;height:32;color:((Math.floor(x/32)+Math.floor(y/32))%2)?"#e020a0":"#10d0e0"}
  }
 }
 D.FilesState {id:service;path:"/home/fixture/Documents";viewMode:"list";persistViewPreferences:true;preferencesLoaded:true}
 D.FilesWindow {id:files;shell:stub;service:service;visible:true}
 PanelWindow {id:elevated;visible:["popover","menu","tooltip"].includes(root.phase);anchors {top:true;right:true}margins {top:140;right:480}implicitWidth:340;implicitHeight:240;exclusionMode:ExclusionMode.Ignore;exclusiveZone:0;color:"transparent";WlrLayershell.layer:WlrLayer.Overlay
  D.PanelSurface {id:panel;anchors.fill:parent;visible:root.phase!=="tooltip"
   D.PanelLabel {x:18;y:18;text:"Elevated controls";font.weight:Font.DemiBold}
   D.ControlRow {x:18;y:54;width:parent.width-36;text:"A readable private action"}
   D.DesktopComboBox {id:choices;x:18;y:100;width:parent.width-36;model:["First choice","Second choice","Third choice"]}
  }
  D.BrightnessControl {id:brightness;x:18;y:18;width:parent.width-36;visible:root.phase==="tooltip"}
 }
 PanelWindow {id:banner;visible:root.phase==="notification";anchors {top:true;right:true}margins {top:90;right:40}implicitWidth:350;implicitHeight:card.height;exclusionMode:ExclusionMode.Ignore;exclusiveZone:0;color:"transparent";WlrLayershell.layer:WlrLayer.Overlay
  D.NotificationCard {id:card;width:parent.width;service:notifications;notification:({id:7,app:"Private fixture",desktop:"",summary:"Readable elevated notification",body:"Synthetic content remains distinct from its background.",icon:"",image:"",timestamp:Date.now()+3600000,actions:[{id:"default",label:"Open"}],urgency:1,resident:true,transient:false,banner:true,unread:true})}
 }
 D.VolumeHud {id:hud;shell:stub;screen:Quickshell.screens[0];outputName:"HEADLESS-1"}
 function color(value){return {r:value.r,g:value.g,b:value.b,a:value.a};}
 function rect(item,owner){const p=item.mapToItem(owner.contentItem,0,0);return {x:p.x,y:p.y,width:item.width,height:item.height};}
 function scan(item,fn){if(fn(item))return item;for(const child of item.children || []){const found=scan(child,fn);if(found)return found;}return null;}
 function same(a,b){if(typeof b==="string")b=Qt.color(b);return a && b && Math.abs(a.r-b.r)<0.0001 && Math.abs(a.g-b.g)<0.0001 && Math.abs(a.b-b.b)<0.0001 && Math.abs(a.a-b.a)<0.0001;}
 function texts(item,owner){let rows=[];function visit(item){if(!item.visible || item.width<=0 || item.height<=0)return;if(item.text!==undefined && item.font!==undefined && item.color!==undefined && String(item.text))rows.push({text:String(item.text),rect:rect(item,owner),color:color(item.color),size:item.font.pixelSize});for(const child of item.children || [])visit(child);}visit(item);return rows;}
 function region(role,item,owner){return {role:role,rect:rect(item,owner),color:color(item.color),texts:texts(item,owner)};}
 IpcHandler {target:"materialHierarchy"
  function open(value:string):void {choices.popup.close();brightness.materialAuditTip.visible=false;root.phase=value;if(value==="menu")Qt.callLater(()=>choices.popup.open());if(value==="tooltip")Qt.callLater(()=>{brightness.materialAuditTip.x=8;brightness.materialAuditTip.y=100;brightness.materialAuditTip.visible=true;});}
  function state():string {
   const document=scan(files.contentItem,item=>same(item.color,D.Theme.content) && item.width===files.width-files.sidebarWidth && item.height===files.height);
   const sidebar=scan(files.contentItem,item=>same(item.color,D.Theme.sidebarMaterial) && item.width===files.sidebarWidth && item.height===files.height);
   const toolbar=scan(files.contentItem,item=>same(item.color,D.Theme.colors.background) && item.x===files.sidebarWidth && item.width===files.width-files.sidebarWidth && item.height<files.height);
   let regions=[];for(const pair of [["document",document],["sidebar",sidebar],["toolbar",toolbar]])if(pair[1])regions.push(region(pair[0],pair[1],files));
   let overlay=null;
   if(root.phase==="popover")overlay=region("popover",panel,elevated);
   if(root.phase==="menu" && choices.popup.visible)overlay=region("menu",choices.popup.background,elevated);
   if(root.phase==="tooltip" && brightness.materialAuditTip.visible)overlay=region("tooltip",brightness.materialAuditTip.background,elevated);
   if(root.phase==="notification")overlay=region("notification",card,banner);
   if(root.phase==="hud"){const surface=scan(hud.contentItem,item=>item.color!==undefined && same(item.color,D.Theme.popoverMaterial));if(surface)overlay=region("hud",surface,hud);}
   if(overlay){const owner=root.phase==="notification"?banner:root.phase==="hud"?hud:elevated;overlay.texts=texts(owner.contentItem,owner);overlay.window={width:owner.width,height:owner.height,top:owner.margins.top,right:owner.margins.right,bottom:owner.margins.bottom};}
   return JSON.stringify({phase:root.phase,dark:D.Theme.dark,ready:service.ready && !service.busy && service.rows.length===1,filesTitle:files.title,regions:regions,overlay:overlay,foreground:color(Qt.color(D.Theme.colors.text)),content:color(D.Theme.content),background:color(Qt.color(D.Theme.colors.background))});
  }
 }
}

import QtQuick
import QtQuick.Controls
import QtQuick.Window
import Quickshell
import Quickshell.Wayland
import Quickshell.Io
import "assets/shell" as D
ShellRoot {
 id:audit
 property string family:""
 property var actions:[]
 property string wanted:""
 QtObject {id:searchService
  property var files:[];property var preview:({});property bool opening:false;property bool searching:false
  property var resourceRetirement:null
  signal opened()
  signal browseRequested(string path)
  function search(query){}function requestPreview(path,token){}function refresh(){}
  function open(path,parent,token,browse){audit.actions=audit.actions.concat({path:path,parent:parent,token:token,browse:browse});}
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

 }
 D.AppSwitcher {id:switcher;shell:stub}
 Item {id:captionMeasures;visible:false
  Repeater {model:switcher.items;D.PanelLabel {required property var modelData;text:modelData.label;font.pixelSize:D.Theme.textRole("label")}}
 }

 function walk(item){let rows=[item];for(const child of item.children || [])rows=rows.concat(walk(child));return rows;}
 function owner(){return family==="switcher"?switcher:layer;}
 function body(){return family==="launcher"?launcher:owner().contentItem;}
 function shown(item,body){for(let parent=item;parent;parent=parent.parent){if(!parent.visible)return false;if(parent===body)return true;}return false;}
 function box(item,body){const at=item.mapToItem(body,0,0);return {x:at.x,y:at.y,width:item.width,height:item.height};}
 function target(){const item=body();return walk(item).find(child=>family==="controls"?child.text==="System Settings…":family==="notifications"?child.objectName==="notification-card":child.Accessible && child.Accessible.name==="Search applications, files, actions and clipboard history");}
 function state(){const window=owner(),item=body(),origin=item.mapToItem(window.contentItem,0,0);return {family:family,dark:D.Theme.dark,reduced:!!D.Theme.preferences.reduceTransparency,visible:window.visible,width:window.width,height:window.height,body:{x:origin.x,y:origin.y,width:item.width,height:item.height},selected:switcher.selected,iconSizes:walk(switcher.contentItem).filter(child=>shown(child,switcher.contentItem) && child.iconSource!==undefined).map(child=>({width:child.width,height:child.height})),tileWidth:switcher.tileWidth,captionAdvanceScale:switcher.captionAdvanceScale,switcherLabel:switcher.items[switcher.selected]?switcher.items[switcher.selected].label:"",captionMetrics:walk(switcher.contentItem).filter(child=>shown(child,switcher.contentItem) && child.text && child.font).map(child=>({text:String(child.text),width:child.width,naturalWidth:child.implicitWidth,probeNaturalWidth:((Array.from(captionMeasures.children).find(probe=>probe.text===child.text)) || {}).implicitWidth || 0,resolvedFamily:child.fontInfo.family})),selectedKey:launcher.selectedKey,currentLabel:launcher.current?launcher.current.title:"",windowActive:window.contentItem.Window.window?window.contentItem.Window.window.active:false,currentKey:launcher.resultKey(launcher.current),captionSize:walk(switcher.contentItem).filter(child=>shown(child,switcher.contentItem) && child.text!==undefined && child.font!==undefined && String(child.text)).map(child=>child.font.pixelSize),targetFocus:target()?target().activeFocus:false,query:family==="launcher"?launcher.queryText:"",launcher:launcher.status(),actions:actions,wanted:wanted,control:(()=>{const control=wanted?walk(item).find(child=>child.Accessible && child.Accessible.name===wanted):null;if(!control)return null;let r=box(control,window.contentItem);for(let p=control.parent;p && p!==window.contentItem;p=p.parent)if(p.clip){const b=box(p,window.contentItem),x=Math.max(r.x,b.x),y=Math.max(r.y,b.y);r={x:x,y:y,width:Math.min(r.x+r.width,b.x+b.width)-x,height:Math.min(r.y+r.height,b.y+b.height)-y};}return {focus:control.activeFocus,pressed:control.pressed===undefined?false:control.pressed,rect:box(control,window.contentItem),clipped:r,contentHeight:control.contentHeight===undefined?null:control.contentHeight,contentY:control.contentY===undefined?null:control.contentY};})(),texts:walk(item).filter(child=>shown(child,item) && child.text!==undefined && child.font!==undefined && child.color!==undefined && String(child.text)).map(child=>({text:String(child.text),color:child.color.toString(),width:child.width,height:child.height,rect:box(child,item),pixelSize:child.font.pixelSize})),rectangles:walk(item).filter(child=>shown(child,item) && child.radius!==undefined && child.color!==undefined && child.width>20 && child.height>15).map(child=>({width:child.width,height:child.height,color:child.color.toString(),alpha:child.color.a,rect:box(child,item)})),focus:walk(window.contentItem).filter(child=>child.activeFocus).map(child=>child.Accessible?child.Accessible.name:"")};}
 IpcHandler {target:"launcherText"
  function configure(mode:string,scale:real):void {D.Theme.colors=JSON.parse(Quickshell.env("LUMEN_FIXTURE_PALETTES"))[mode];D.Theme.preferences={textScale:scale,reduceTransparency:true,reduceMotion:true};}
  function layout(value:string):void {launcher.setProvider("Applications");launcher.gridMode=value==="grid";launcher.previewOpen=value==="preview";}
  function focusLabel(label:string):void {audit.wanted=label;const item=walk(body()).find(child=>child.Accessible && child.Accessible.name===label);if(!item)throw Error("Missing actual preview control "+label);item.forceActiveFocus();}
  function resetEntries():void {stub.windows=[{appId:"fixture.first",title:"First document",id:101,pid:11},{appId:"fixture.second",title:"Second document",id:102,pid:12}];stub.entries=[{id:"fixture.first",name:"First application",icon:"application-x-executable",noDisplay:false},{id:"fixture.second",name:"Second application",icon:"application-x-executable",noDisplay:false}];}
  function manySwitcher():void {audit.actions=[];audit.wanted="";layer.visible=false;switcher.visible=false;audit.family="switcher";stub.entries=Array.from({length:8},(_,i)=>({id:"fixture."+i,name:"Application "+(i+1),noDisplay:false}));stub.windows=stub.entries.map((entry,i)=>({appId:entry.id,title:entry.name+" document",id:201+i,pid:101+i}));switcher.command("begin");}
  function select(index:int):void {launcher.selectedIndex=index;}
  function removeSelected():void {const key=launcher.current.id;stub.entries=stub.entries.filter(item=>item.id!==key);}
  function longPreview():void {searchService.preview={type:"text",text:Array.from({length:40},(_,i)=>"Line "+(i+1)+": readable bounded private preview content.").join("\n")+"\nFINAL PRIVATE PREVIEW LINE",truncated:true};}
  function openFiles():void {searchService.files=[{kind:"file",id:"private-file",path:"/home/fixture/document.txt",title:"Private document",subtitle:"Private text file",token:"private-token",size:42,mtime:0}];launcher.setProvider("Files");launcher.previewOpen=true;}

  function open(value:string):void {resetEntries();audit.actions=[];audit.wanted="";layer.visible=false;switcher.visible=false;audit.family=value;if(value==="switcher")switcher.command("begin");else {layer.visible=true;if(value==="launcher"){launcher.setProvider("Applications");Qt.callLater(launcher.focusSearch);}}}
  function focusTarget():bool{const item=audit.target();if(!item)return false;item.forceActiveFocus();return item.activeFocus;}
  function status():string{return JSON.stringify(audit.state());}
 }
}

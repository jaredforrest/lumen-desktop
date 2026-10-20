import QtQuick
import QtQuick.Controls
import QtQuick.Window
import Quickshell
import "assets/shell" as D
ShellRoot {
 id:audit
 property var cases:CASES
 property var palettes:PALETTES
 property var actions:[]
 property var records:[]
 property int index:0
 property int phase:0
 property int stage:0
 property bool painting:false
 property var target:null
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
  function activate(entry){throw Error("No external app activation");}function focusWindow(window){audit.actions=audit.actions.concat(window.id);}
  function iconPath(icon){return "";}function openSettings(page){audit.actions=audit.actions.concat("settings");}
 }

 FloatingWindow {id:window;visible:false;implicitWidth:960;implicitHeight:720
  Rectangle {id:surface;anchors.fill:parent;color:D.Theme.colors.background
   D.AppLauncher {id:launcher;shell:stub;anchors.centerIn:parent;onCloseRequested:visible=false}
  }
 }
 D.PanelLabel {id:probe;visible:false;font:target?target.font:Qt.font({family:palettes.font,pixelSize:12});text:"0123456789"}
 function check(value,message){if(!value)throw Error(message+" "+JSON.stringify(cases[index])+" phase="+phase);}
 function walk(item,seen){if(!item || seen.includes(item))return [];seen.push(item);let result=[item];for(const child of Array.from(item.children || []).concat(Array.from(item.resources || [])))result=result.concat(walk(child,seen));if(item.contentItem)result=result.concat(walk(item.contentItem,seen));return result;}
 function shown(item){while(item && item!==launcher){if(!item.visible)return false;item=item.parent;}return item===launcher;}
 function rect(item){const at=item.mapToItem(surface,0,0);return {x:at.x,y:at.y,width:item.width,height:item.height};}
 Timer {interval:20;repeat:true;running:true;onTriggered:{try{
  if(painting)return;if(index===cases.length){console.log("LAUNCHER_TEXT_RESULT "+JSON.stringify(records));Qt.quit();return;}
  const c=cases[index];
  if(stage===0){launcher.visible=false;window.visible=false;D.Theme.font=palettes.font;D.Theme.colors=palettes[c.mode];D.Theme.preferences={textScale:c.scale,reduceMotion:true,reduceTransparency:true};window.implicitWidth=c.small?480:960;window.implicitHeight=c.small?360:720;window.visible=true;launcher.visible=true;launcher.setProvider("Applications");launcher.gridMode=c.layout==="grid";launcher.previewOpen=c.layout==="preview";stage=1;return;}
  const items=walk(launcher,[]).filter(item=>typeof item.mapToItem==="function" && shown(item));
  if(stage===1){
   if(phase===0)target=items.find(item=>item.Accessible.name==="Search applications, files, actions and clipboard history");
   if(phase===1)target=items.find(item=>item.font && item.text==="First application" && !item.clicked && (c.layout!=="preview" || item.font.weight===Font.DemiBold));
   if(phase===2)target=items.find(item=>item.font && item.text==="↑↓ to select   Return to open   Ctrl+Tab for categories   Escape to close");
   check(target,"Production text missing");if(phase===0 && target.text!=="application"){target.text="application";return;}stage=2;return;
  }
  if(stage===2){
   const expected=phase===0?24:phase===1?(c.layout==="grid"?11:c.layout==="preview"?15:14):10;check(target.font.pixelSize===D.Theme.textSize(expected),"Essential launcher text ignores scale actual="+target.font.pixelSize+" expected="+D.Theme.textSize(expected));check(probe.fontInfo.family===palettes.font,"Resolved font mismatch");check(launcher.height<=window.height-31,"Launcher exceeds available height");const box=rect(target);check(box.x>=0 && box.y>=0 && box.x+box.width<=window.width+1 && box.y+box.height<=window.height+1,"Essential text outside window");check(target.contentHeight<=target.height+1,"Essential text internally clipped");
   let roleMetrics=[];
   for(const name of launcher.providerChoices){const label=items.find(item=>item.font && item.text===name);check(label && label.font.pixelSize===D.Theme.textSize(11),"Filter text role "+name);const b=rect(label);check(b.x>=0 && b.y>=0 && b.x+b.width<=window.width+1 && b.y+b.height<=window.height+1,"Filter outside window "+name);roleMetrics.push({role:"filter",text:name,pixelSize:label.font.pixelSize,rect:b});}
   if(c.layout!=="grid")for(const label of items.filter(item=>item.font && item.text==="Application")){check(label.font.pixelSize===D.Theme.textSize(10),"Result metadata role");roleMetrics.push({role:"metadata",text:label.text,pixelSize:label.font.pixelSize,rect:rect(label)});}
   if(c.layout==="preview"){const body=items.find(item=>item.font && item.text==="Open this application with Return.");check(body && body.font.pixelSize===D.Theme.textSize(12),"Preview body text role");roleMetrics.push({role:"previewBody",text:body.text,pixelSize:body.font.pixelSize,rect:rect(body)});}
   const row={entry:c,phase:["search","result","footer"][phase],text:target.text,pixelSize:target.font.pixelSize,color:String(target.color),textBounds:box,launcher:rect(launcher),roleMetrics:roleMetrics,resolvedFamily:probe.fontInfo.family};painting=true;check(surface.grabToImage(result=>{check(result.saveToFile("OUTPUT/"+index+"-"+phase+".png"),"Paint save");records.push(row);phase++;if(phase===3){phase=0;index++;stage=0;}else stage=1;painting=false;},Qt.size(surface.width,surface.height)),"Actual launcher paint");
  }
 }catch(error){console.error("LAUNCHER_TEXT_FAILURE "+error);Qt.quit();}}}
}
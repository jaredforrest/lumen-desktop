import QtQuick
import QtQuick.Controls
import QtQuick.Window
import Quickshell
import "desktop" as D
ShellRoot {
 id:audit
 property var cases:__CASES__
 property var palettes:__PALETTES__
 property var records:[]
 property int index:0
 property int stage:0
 property bool painting:false
 property var primary:null
 property var target:null
 property var owner:null
 property var tooltip:null
 property var paintItem:null
 FloatingWindow {id:window;visible:false;implicitWidth:500;implicitHeight:400;color:"transparent"
  Rectangle {id:surface;anchors.fill:parent;color:D.Theme.colors.background
   D.FocusFlickable {id:viewport;anchors.fill:parent;anchors.margins:16;contentWidth:width;contentHeight:column.height;clip:true;ScrollBar.vertical:D.DesktopScrollBar {}
    Column {id:column;width:parent.width;spacing:12
     D.AudioDeviceControl {id:audio;visible:false;width:parent.width;node:({id:1,nickname:"Private synthetic playback device",audio:{volume:.5,muted:false}});error:"Synthetic sound setting could not be changed. Select the intended device and try again."}
     D.BrightnessControl {id:brightness;visible:false;width:parent.width}
     D.DateTimeField {id:date;visible:false;width:parent.width;label:"Starts";text:"2026-09-28T23:59"}
     D.DesktopSpinBox {id:spin;visible:false;width:100;from:0;to:9999;value:9999;editable:true;Accessible.name:"Private numeric value"}
    }
   }
  }
 }
 QtObject {id:stub;property date currentDate:new Date(2026,8,28);function claimPopup(name){}function focusOwnedWindow(name){}function openSettings(name){}}
 D.CalendarWindow {id:calendar;shell:stub}
 D.PanelLabel {id:fontProbe;visible:false;font:audit.primary?audit.primary.font:Qt.font({family:palettes.font,pixelSize:12});text:"0123456789"}
 function check(value,message){if(!value)throw Error(message+" "+JSON.stringify(cases[index]));}
 function walk(item,seen){if(!item || seen.includes(item))return [];seen.push(item);let result=[item];for(const child of Array.from(item.children || []).concat(Array.from(item.resources || [])))result=result.concat(walk(child,seen));if(item.contentItem)result=result.concat(walk(item.contentItem,seen));return result;}
 function shown(item){while(item && item!==owner.contentItem){if(!item.visible)return false;item=item.parent;}return item===owner.contentItem;}
 function rect(item){const p=item.mapToItem(owner.contentItem,0,0);return {x:p.x,y:p.y,width:item.width,height:item.height};}
 function bounds(item){let r=rect(item);for(let p=item.parent;p && p!==owner.contentItem;p=p.parent){if(p.clip){const b=rect(p),x=Math.max(r.x,b.x),y=Math.max(r.y,b.y);r={x:x,y:y,width:Math.min(r.x+r.width,b.x+b.width)-x,height:Math.min(r.y+r.height,b.y+b.height)-y};}}return r;}
 Timer {interval:20;repeat:true;running:true;onTriggered:{try{
  if(audit.painting)return;
  if(audit.index===audit.cases.length){console.log("SHARED_TEXT_RESULT "+JSON.stringify(records));Qt.quit();return;}
  const c=cases[index];
  if(stage===0){if(tooltip)tooltip.visible=false;window.visible=false;calendar.inspector=false;calendar.calendarState.editing=false;calendar.visible=false;audio.visible=false;brightness.visible=false;date.visible=false;spin.visible=false;D.Theme.font=palettes.font;D.Theme.colors=palettes[c.mode];D.Theme.preferences={textScale:c.scale,reduceMotion:true,reduceTransparency:true};audit.owner=c.consumer==="CalendarEditor"?calendar:window;owner.implicitWidth=c.consumer==="CalendarEditor"?(c.small?480:900):(c.small?260:500);owner.implicitHeight=c.consumer==="CalendarEditor"?(c.small?320:700):400;owner.visible=true;if(c.consumer==="Audio")audio.visible=true;else if(c.consumer==="Brightness")brightness.visible=true;else if(c.consumer==="Date")date.visible=true;else if(c.consumer==="Spin")spin.visible=true;audit.stage=1;return;}
  const items=walk(owner.contentItem,[]);
  if(stage===1){
   if(c.consumer==="CalendarEditor"){if(!calendar.calendarState.loaded || calendar.calendarState.busy)return;calendar.create(calendar.selected);audit.stage=2;return;}
   if(c.consumer==="Audio"){audit.primary=items.find(item=>item.text===audio.label && item.font);audit.target=items.find(item=>typeof item.mapToItem==="function" && item.Accessible.name==="Mute "+audio.label);}
   if(c.consumer==="Brightness"){audit.tooltip=items.find(item=>item.text && item.contentItem && item.padding===7 && typeof item.open==="function");check(tooltip,"Actual brightness tooltip missing");tooltip.visible=true;audit.primary=tooltip.contentItem;audit.target=items.find(item=>typeof item.mapToItem==="function" && item.Accessible.name==="Display brightness");}
   if(c.consumer==="Date"){audit.target=items.find(item=>typeof item.mapToItem==="function" && item.Accessible.name==="Starts year");audit.primary=target.contentItem;}
   if(c.consumer==="Spin"){audit.target=spin;audit.primary=spin.contentItem;}
   check(primary && target,"Production control missing");target.forceActiveFocus();audit.stage=3;return;
  }
  if(stage===2){const initial=items.find(item=>typeof item.mapToItem==="function" && item.Accessible.name==="Event title" && shown(item));if(!initial || !initial.activeFocus)return;audit.target=items.find(item=>typeof item.mapToItem==="function" && item.Accessible.name==="Starts year" && shown(item));check(target,"Actual parent date field missing");audit.primary=target.contentItem;target.forceActiveFocus();audit.stage=3;return;}
  if(stage===4){audit.target=items.find(item=>item.text==="Cancel" && item.clicked && shown(item));check(target,"Calendar editor Cancel missing");audit.primary=target.contentItem;target.forceActiveFocus();audit.stage=5;return;}
  if(stage===3 || stage===5){
   if(c.consumer==="Brightness" && !tooltip.opened)return;
   check(target.activeFocus,"Essential control focus target="+target+" active="+owner.contentItem.Window.window.activeFocusItem);const b=bounds(target);check(b.width>=target.width-1 && b.height>=target.height-1 && b.x>=0 && b.y>=0 && b.x+b.width<=owner.width+1 && b.y+b.height<=owner.height+1,"Essential control clipped "+JSON.stringify(b));const expected=c.consumer==="Audio"?12:c.consumer==="Brightness"?11:c.consumer==="CalendarEditor" && stage===5?13:12;check(primary.font.pixelSize===D.Theme.textSize(expected),"Essential text ignores scale: "+primary.font.pixelSize);check(fontProbe.fontInfo.family===palettes.font,"Resolved font mismatch "+fontProbe.fontInfo.family);
   if(["Date","Spin"].includes(c.consumer) || (c.consumer==="CalendarEditor" && stage===3))check(primary.contentWidth<=primary.width+1,"Numeric content clipped");
   if(c.consumer==="Brightness"){const at=tooltip.mapToItem?rect(tooltip):rect(primary),b=rect(primary);check(b.x>=0 && b.y>=0 && b.x+b.width<=owner.width+1 && b.y+b.height<=owner.height+1,"Tooltip outside window "+JSON.stringify(b));}
   if(c.consumer==="CalendarEditor")for(const name of ["Event title","Event location","Event notes"]){const field=items.find(item=>typeof item.mapToItem==="function" && item.Accessible.name===name && shown(item));check(field && field.font.pixelSize===D.Theme.textSize(12),"Editor text role "+name);}
   const r=rect(primary);audit.paintItem=c.consumer==="Brightness"?primary:c.consumer==="CalendarEditor"?Array.from(calendar.contentItem.children).find(item=>item.visible && item.width===calendar.width && item.height===calendar.height):surface;const phase=c.consumer==="CalendarEditor"?(stage===3?"date":"footer"):"control";const record={entry:c,phase:phase,target:b,text:primary.text,textBounds:r,color:String(primary.color),pixelSize:primary.font.pixelSize,resolvedFamily:fontProbe.fontInfo.family,textOnly:c.consumer==="Brightness"};audit.painting=true;check(paintItem.grabToImage(result=>{if(!result.saveToFile("__OUTPUT__/"+index+"-"+phase+".png")){console.error("SHARED_TEXT_FAILURE save");Qt.quit();return;}records.push(record);if(c.consumer==="CalendarEditor" && stage===3)stage=4;else{index++;stage=0;}painting=false;},Qt.size(paintItem.width,paintItem.height)),"Actual control paint");
  }
 }catch(error){console.error("SHARED_TEXT_FAILURE "+error);Qt.quit();}}}
}

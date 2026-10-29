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
 property var phases:["date","metadata","notes","status","action","provider"]
 property int index:0
 property int phase:0
 property int stage:0
 property bool painting:false
 property var target:null
 QtObject {id:stub;property date currentDate:new Date(2026,8,28);property int accounts:0;function claimPopup(name){}function focusOwnedWindow(name){}function openSettings(name){accounts++;}}
 D.CalendarWindow {id:owner;shell:stub}
 D.PanelLabel {id:probe;visible:false;font:target?(target.clicked?target.contentItem.font:target.font):Qt.font({family:palettes.font,pixelSize:12});text:"0123456789"}
 function check(value,message){if(!value)throw Error(message+" "+JSON.stringify(cases[index])+" phase="+phases[phase]);}
 function walk(item,seen){if(!item || seen.includes(item))return [];seen.push(item);let result=[item];for(const child of Array.from(item.children || []).concat(Array.from(item.resources || [])))result=result.concat(walk(child,seen));if(item.contentItem)result=result.concat(walk(item.contentItem,seen));return result;}
 function shown(item){while(item && item!==owner.contentItem){if(!item.visible)return false;item=item.parent;}return item===owner.contentItem;}
 function rect(item,relative){const at=item.mapToItem(relative || owner.contentItem,0,0);return {x:at.x,y:at.y,width:item.width,height:item.height};}
 function bounds(item){let r=rect(item);for(let p=item.parent;p && p!==owner.contentItem;p=p.parent)if(p.clip){const b=rect(p),x=Math.max(r.x,b.x),y=Math.max(r.y,b.y);r={x:x,y:y,width:Math.min(r.x+r.width,b.x+b.width)-x,height:Math.min(r.y+r.height,b.y+b.height)-y};}return r;}
 Timer {interval:20;repeat:true;running:true;onTriggered:{try{
  if(painting)return;
  if(index===cases.length){console.log("DETAIL_TEXT_RESULT "+JSON.stringify(records));Qt.quit();return;}
  const c=cases[index],card=owner.calendarState;
  if(stage===0){owner.visible=false;owner.inspector=false;card.editing=false;card.detail=null;D.Theme.font=palettes.font;D.Theme.colors=palettes[c.mode];D.Theme.preferences={textScale:c.scale,reduceMotion:true,reduceTransparency:true};owner.implicitWidth=c.small?480:900;owner.implicitHeight=c.small?320:700;owner.visible=true;stage=1;return;}
  if(stage===1){if(!card.loaded || card.busy || !card.events.length)return;const event=Object.assign({},card.events[0],{editable:!c.readonly});owner.eventDetails(event,new Date(2026,8,28));card.optionsOpen=true;stage=2;return;}
  const items=walk(owner.contentItem,[]).filter(item=>typeof item.mapToItem==="function" && shown(item));
  if(stage===2){
   const name=phases[phase];
   if(name==="date")target=items.find(item=>item.font && item.text===Qt.formatDateTime(card.selected,Qt.locale().dateFormat(Locale.ShortFormat)));
   if(name==="metadata")target=items.find(item=>item.font && item.text==="Google · Private calendar · Room 42, north wing");
   if(name==="notes")target=items.find(item=>item.font && item.text==="Private notes: bring the printed agenda and review the schedule.");
   if(name==="status")target=items.find(item=>item.font && item.text===(c.readonly?"Some event details require Google Calendar; use Open Google below.":"Changes apply only to this occurrence."));
   if(name==="action")target=items.find(item=>item.text==="New Event…" && item.clicked);
   if(name==="provider")target=items.find(item=>item.font && item.text==="Private provider · Waiting for first sync");
   check(target,"Actual details item missing");
   const viewport=items.find(item=>item.objectName==="calendar-inspector-content");check(viewport,"Production inspector scroll missing");
   if(name==="action")target.forceActiveFocus();else{viewport.forceActiveFocus();const at=rect(target,viewport.contentItem);viewport.contentY=Math.max(0,Math.min(viewport.contentHeight-viewport.height,at.y-8));}
   stage=3;return;
  }
  if(stage===3){
   const name=phases[phase],text=name==="action"?target.contentItem:target,base=name==="date" || name==="action"?13:name==="notes"?12:name==="provider"?10:11;
   check(text.font.pixelSize===D.Theme.textSize(base),"Essential details text ignores scale actual="+text.font.pixelSize+" expected="+D.Theme.textSize(base));check(probe.fontInfo.family===palettes.font,"Resolved family "+probe.fontInfo.family);
   const b=bounds(target);check(b.width>=target.width-1 && b.height>=target.height-1 && b.x>=0 && b.y>=0 && b.x+b.width<=owner.width+1 && b.y+b.height<=owner.height+1,"Details item clipped "+JSON.stringify(b));
   if(name==="action")check(target.activeFocus,"Details action focus lost");else check(text.contentWidth<=text.width+1 && text.contentHeight<=text.height+1,"Details text internal clipping");
   const edit=items.find(item=>item.text==="Edit Occurrence…" && item.clicked),remove=items.find(item=>item.text==="Delete…" && item.clicked);check(edit && remove && edit.enabled===!c.readonly && remove.enabled===!c.readonly,"Read-only action eligibility changed edit="+(edit?edit.enabled:"missing")+" remove="+(remove?remove.enabled:"missing")+" detailEditable="+(card.detail?card.detail.editable:"missing")+" busy="+card.busy);
   const record={entry:c,phase:name,text:text.text,pixelSize:text.font.pixelSize,color:String(text.color),textBounds:rect(text),target:b,resolvedFamily:probe.fontInfo.family};const painted=Array.from(owner.contentItem.children).find(item=>item.visible && item.width===owner.width && item.height===owner.height);check(painted,"Production paint surface missing");painting=true;check(painted.grabToImage(result=>{check(result.saveToFile("__OUTPUT__/"+index+"-"+name+".png"),"Paint save");records.push(record);phase++;if(phase===phases.length){phase=0;index++;stage=0;}else stage=2;painting=false;},Qt.size(painted.width,painted.height)),"Actual details paint");
  }
 }catch(error){console.error("DETAIL_TEXT_FAILURE "+error);Qt.quit();}}}
}

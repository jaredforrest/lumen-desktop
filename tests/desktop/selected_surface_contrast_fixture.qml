import QtQuick
import Quickshell
import "LUMEN_ASSETS" as D
ShellRoot {
 id:root
 property var cases:LUMEN_CASES
 property int index:0
 D.CalendarCard {id:calendar;today:new Date(2000,0,21);selected:new Date(2000,0,20);active:false}
 D.AppPreview {id:preview;kind:"Files";listMode:true;selectedFile:0;files:[{name:"Selected fixture",directory:false}]}
 function find(node,text) {
  if(node.objectName===text)return node;
  if(node.text===text && node.color!==undefined)return node;
  const children=node.children || [];
  for(let i=0;i<children.length;i++){const value=find(children[i],text);if(value)return value;}
  return null;
 }
 Timer {interval:30;running:true;repeat:true;onTriggered:{try {
  if(root.index>=root.cases.length){console.log("SELECTED_CONTRAST_PASS "+root.cases.length);Qt.quit();return;}
  const entry=root.cases[root.index];D.Theme.colors=entry.colors;
  preview.files=[{name:"Selected fixture",directory:false}];preview.selectedFile=0;
  const targets=[[preview,"Selected fixture"],[preview,"Document"]];
  for(const target of targets){const item=root.find(target[0],target[1]);if(!item)throw Error("Missing actual control: "+target[1]);if(String(item.color)!==String(Qt.color(entry.colors.selectedText)))throw Error(entry.mode+"/"+entry.accent+" "+target[1]+" foreground "+item.color+" expected "+entry.colors.selectedText);}
  const detail=root.find(preview,"Document"), glyph=Array.from(detail.parent.children).find(item=>item.ink!==undefined);
  if(String(detail.parent.color)!==String(Qt.color(entry.colors.accent)) || !glyph || String(glyph.ink)!==String(Qt.color(entry.colors.selectedText)))throw Error("Selected row fill/icon bypass resolved palette");
  preview.selectedFile=-1;
  if(String(detail.color)!==String(Qt.color(entry.colors.muted)))throw Error("Unselected detail retains selected foreground");
  D.Theme.preferences=Object.assign({},D.Theme.preferences,{mode:entry.mode});calendar.followingToday=false;calendar.selected=new Date(2000,0,20);
  function luminance(color){const channels=[color.r,color.g,color.b].map(value=>value<=0.04045?value/12.92:Math.pow((value+0.055)/1.055,2.4));return channels[0]*0.2126+channels[1]*0.7152+channels[2]*0.0722;}
  for(const mono of [false,true]){calendar.monochrome=mono;for(const day of ["2000-01-20","2000-01-21"]){const cell=root.find(calendar,"calendar-day-"+day);if(!cell)throw Error("Missing calendar day "+day);const a=luminance(cell.contentItem.color),b=luminance(cell.background.children[0].color),ratio=(Math.max(a,b)+0.05)/(Math.min(a,b)+0.05);if(ratio<4.5)throw Error("Calendar filled day "+day+" foreground="+cell.contentItem.color+" fill="+cell.background.children[0].color+" contrast "+ratio+" in "+entry.mode+"/"+entry.accent+" monochrome="+mono);}}
  root.index++;
 }catch(error){console.error("SELECTED_CONTRAST_FAIL "+error);Qt.quit();}}}
}

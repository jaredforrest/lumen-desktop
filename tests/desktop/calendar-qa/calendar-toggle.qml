import QtQuick
import Quickshell
import "source" as D
ShellRoot {
 id:root
 property string outputDirectory: OUTPUT_DIRECTORY_JSON
 property var records:[]
 property int step:0
 property int stage:0
 property bool painting:false
 property var cases:[{mode:"light",contrast:false},{mode:"light",contrast:true},{mode:"dark",contrast:false},{mode:"dark",contrast:true}]
 property var palettes: PALETTES_JSON
 QtObject {id:stub;property date currentDate:new Date(2026,9,20);function claimPopup(name){}function focusOwnedWindow(name){}function openSettings(name){}}
 D.CalendarWindow {id:window;shell:stub}
 Component.onCompleted:{window.calendarState.active=false;window.visible=true;}
 function find(item,predicate){if(predicate(item))return item;for(const child of item.children || []){const found=find(child,predicate);if(found)return found;}return null;}
 function rgba(value){return {r:value.r,g:value.g,b:value.b,a:value.a};}
 function luminance(c){return [c.r,c.g,c.b].map(v=>v<=.04045?v/12.92:Math.pow((v+.055)/1.055,2.4)).reduce((s,v,i)=>s+v*[.2126,.7152,.0722][i],0);}
 function ratio(a,b){const x=luminance(a),y=luminance(b);return (Math.max(x,y)+.05)/(Math.min(x,y)+.05);}
 function rows(){const found=[];for(let i=0;i<100;i++){const name="Calendar "+i,color=window.tint({provider:"Private",calendar:name});if(!found.some(row=>row.color===String(color)))found.push({name:name,color:String(color)});if(found.length===5)break;}if(found.length!==5)throw Error("Expected all five actual calendar tints");return found;}
 function button(name){return find(window.contentItem,item=>item.checkable===true && item.modelData && item.modelData.name===name);}
 Timer {interval:30;repeat:true;running:true;onTriggered:{try {
  if(root.painting)return;
  if(root.step===root.cases.length){console.log("CALENDAR_TOGGLE_RESULT "+JSON.stringify(root.records));Qt.quit();return;}
  const entry=root.cases[root.step];
  if(root.stage===0){D.Theme.colors=root.palettes[entry.mode];D.Theme.preferences={reduceMotion:true,increaseContrast:entry.contrast};window.hiddenCalendars=[];window.calendarState.providers=[{name:"Private",calendars:root.rows().map(row=>({name:row.name}))}];root.stage=1;return;}
  const samples=[];
  for(const row of root.rows()){
   const control=root.button(row.name);if(!control)return;
   const mark=root.find(control.contentItem,item=>item.color!==undefined && item.width===13 && item.height===13);
   const glyph=mark?root.find(mark,item=>item.text==="✓"):null;if(!mark || !glyph || !control.checked)throw Error("Missing actual checked calendar mark");
   const contrast=root.ratio(glyph.color,mark.color);if(contrast<4.5)throw Error("Calendar checked mark "+entry.mode+" "+mark.color+" foreground "+glyph.color+" contrast "+contrast);
   const point=mark.mapToItem(window.contentItem,0,0);samples.push({name:row.name,fill:root.rgba(mark.color),foreground:root.rgba(glyph.color),contrast:contrast,rect:{x:point.x,y:point.y,width:mark.width,height:mark.height}});
  }
  root.painting=true;
  const paintItem=Array.from(window.contentItem.children).find(item=>item.visible && item.width===window.width && item.height===window.height);if(!paintItem)throw Error("No production QML content item");
  if(!paintItem.grabToImage(result=>{try {
   const file=root.outputDirectory+"/"+entry.mode+"-"+entry.contrast+".png";if(!result.saveToFile(file))throw Error("Actual frame capture failed");
   const first=root.button(root.rows()[0].name);first.clicked();const mark=root.find(first.contentItem,item=>item.color!==undefined && item.width===13 && item.height===13);if(first.checked || root.find(mark,item=>item.text==="✓") || mark.color.a!==0)throw Error("Unchecked calendar retained filled mark");
   root.records=root.records.concat([{mode:entry.mode,increaseContrast:entry.contrast,samples:samples,capture:file,unchecked:true}]);root.step++;root.stage=0;root.painting=false;
  }catch(error){console.error("CALENDAR_TOGGLE_FAILURE "+error);Qt.quit();}},Qt.size(window.width*2,window.height*2)))throw Error("Actual frame request rejected");
 }catch(error){console.error("CALENDAR_TOGGLE_FAILURE "+error);Qt.quit();}}}
}
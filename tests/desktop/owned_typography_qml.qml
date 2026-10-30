import QtQuick
import QtQuick.Controls
import QtQuick.Window
import Quickshell
import "desktop" as D
ShellRoot {
 id:audit
 property var cases:__CASES__
 property var records:[]
 property var findings:[]
 property int index:0
 property int stage:0
 property bool painting:false
 property string expectedFamily:__FAMILY__
 property int ticks:0
 property var focusTarget:null
 property var palettes:__PALETTES__
 QtObject {id:exported;property var state:({appName:"",title:"",windowId:0})}
 QtObject {id:stub;property date currentDate:new Date(2026,9,20);property int focusEpoch:0;property var windows:[];property var exportedMenu:exported;function claimPopup(name){}function focusOwnedWindow(name){}function focusWindow(window){}function activeFiles(window){}function openSettings(name){}}
 D.FilesState {id:service;persistViewPreferences:false;path:"__DOCUMENTS__";viewMode:"list"}
 D.FilesWindow {id:files;shell:stub;service:service}
 D.CalendarWindow {id:calendar;shell:stub}
 function check(value,message){if(!value)throw Error(message+" case="+JSON.stringify(cases[index]));}
 function walk(item){let result=[item];for(const child of item.children || [])result=result.concat(walk(child));return result;}
 function visible(item,stop){while(item && item!==stop){if(!item.visible)return false;item=item.parent;}return item===stop;}
 function text(root,value){return walk(root.contentItem).find(item=>item.text===value && visible(item,root.contentItem));}
 function ratio(a,b){const lum=c=>[c.r,c.g,c.b].map(v=>v<=.04045?v/12.92:Math.pow((v+.055)/1.055,2.4)).reduce((sum,v,i)=>sum+v*[.2126,.7152,.0722][i],0),x=lum(a),y=lum(b);return (Math.max(x,y)+.05)/(Math.min(x,y)+.05);}
 function paint(root){return Array.from(root.contentItem.children).find(item=>item.visible && item.width===root.width && item.height===root.height);}
 Component.onCompleted:{

  calendar.calendarState.active=false;calendar.calendarState.loaded=true;calendar.calendarState.monthEvents=[{id:"synthetic",provider:"Private",calendar:"Calendar collection",title:"Planning session",date:"2026-10-20",endDate:"2026-10-20",startTime:"09:00",endTime:"10:00",allDay:false},...[1,2,3].map(index=>({id:"all-day-"+index,provider:"Private",calendar:"Calendar collection",title:"All-day "+index,date:"2026-10-20",endDate:"2026-10-20",startTime:"00:00",endTime:"23:59",allDay:true}))];calendar.calendarState.providers=[{name:"Private",calendars:[{name:"Calendar collection"}]}];service.navigate("__DOCUMENTS__");
 }
 Timer {interval:20;repeat:true;running:true;onTriggered:{try{
  if(++audit.ticks>1200)throw Error("Bounded presentation deadline");if(audit.painting)return;
  if(audit.index===audit.cases.length){
   if(audit.findings.length){console.error("OWNED_TYPOGRAPHY_FAILURE "+JSON.stringify(audit.findings));Qt.quit();return;}
   console.log("OWNED_TYPOGRAPHY_RESULT "+JSON.stringify(audit.records));Qt.quit();return;
  }
  if(!service.ready || service.busy)return;
  const entry=audit.cases[audit.index],window=entry.app==="Files"?files:calendar;
  if(audit.stage===0){files.visible=entry.app==="Files";calendar.visible=entry.app==="Calendar";D.Theme.colors=audit.palettes[entry.mode];D.Theme.preferences={textScale:entry.scale,reduceMotion:true};if(entry.app==="Files")service.viewMode=entry.view;else calendar.view=entry.view;window.implicitWidth=entry.width;window.implicitHeight=entry.height;audit.stage=1;return;}
  if(audit.stage===1){const native=window.contentItem.Window.window;if(!native)return;native.width=entry.width;native.height=entry.height;audit.stage=2;return;}
  if(window.width!==entry.width || window.height!==entry.height)return;
  const label=audit.text(window,entry.app==="Files"?"An ordinary document.txt":"Calendar collection");check(label,"Actual production primary label");check(label.fontInfo.family===audit.expectedFamily,"Requested family unresolved: "+label.fontInfo.family+" expected "+audit.expectedFamily);check(label.font.pixelSize===Math.round((entry.app==="Files" && entry.view==="gallery"?11:12)*entry.scale),"Primary label ignored text scale: "+label.font.pixelSize);
  const at=label.mapToItem(window.contentItem,0,0);check(label.height>=label.font.pixelSize,"Primary label clipped vertically");
  const focusable=audit.walk(window.contentItem).filter(item=>item.activeFocusOnTab && item.enabled && audit.visible(item,window.contentItem));check(focusable.length>0,"Reachable owned controls");
  if(audit.stage===2){
   if(entry.app==="Calendar" && entry.view==="Month"){
    const days=audit.walk(window.contentItem).filter(item=>item.day!==undefined && item.inMonth!==undefined && item.activeFocusOnTab);check(days.length>=28,"Production date keyboard controls");audit.focusTarget=days[days.length-1];audit.focusTarget.forceActiveFocus();
   }else if(entry.app==="Files"){
    if(entry.view==="list"){const header=audit.text(window,"Name ↑");check(header && header.height>=header.font.pixelSize,"Files heading fits");}if(entry.view==="list")check(label.parent.height>=label.height+8,"File row text has vertical space");audit.focusTarget=audit.walk(window.contentItem).find(item=>item.placeholderText==="Filter folder or tag:red");check(audit.focusTarget,"Production filter");audit.focusTarget.forceActiveFocus();
   }else {audit.focusTarget=entry.view==="Year"?audit.text(window,"Today").parent:audit.walk(window.contentItem).find(item=>item.modelData && item.modelData.event && item.modelData.event.title==="Planning session");check(audit.focusTarget,"Actual timed event keyboard target");audit.focusTarget.forceActiveFocus();}
   audit.stage=3;return;
  }
  check(audit.focusTarget.activeFocus,"Focused production control retained");
  if(entry.app==="Calendar" && entry.view!=="Year" && audit.stage===3){
   const viewport=audit.walk(window.contentItem).find(item=>item.objectName==="calendar-content-viewport"),f=audit.focusTarget,point=f.mapToItem(viewport,0,0);check(point.x>=-1 && point.y>=-1 && point.x+f.width<=viewport.width+1 && point.y+f.height<=viewport.height+1,"Focused calendar control revealed within viewport: "+JSON.stringify({x:point.x,y:point.y,width:f.width,height:f.height,viewportWidth:viewport.width,viewportHeight:viewport.height}));
   const tabs=["Day","Week","Month","Year"].map(value=>audit.text(window,value));for(const tab of tabs)check(tab && tab.width>=tab.contentWidth,"Calendar view title has space");
  }
  if(entry.app==="Calendar" && audit.stage===3){audit.focusTarget=audit.text(window,"Accounts…").parent;audit.focusTarget.forceActiveFocus();audit.stage=4;return;}
  if(entry.app==="Calendar"){
   const viewport=audit.walk(window.contentItem).find(item=>item.objectName==="calendar-sidebar-viewport"),item=audit.focusTarget,point=item.mapToItem(viewport,0,0);if(point.y < -1 || point.y+item.height>viewport.height+1)audit.findings.push({case:entry,problem:"focused sidebar command outside viewport",y:point.y,height:item.height,viewportHeight:viewport.height});
  }
  if(entry.app==="Calendar" && entry.view==="Year"){
   const controls=audit.walk(window.contentItem).filter(item=>item.valid===true && item.number!==undefined && audit.visible(item,window.contentItem));check(controls.length>300,"Actual year date buttons");
   const small=controls.find(item=>{const label=audit.walk(item).find(child=>child.font && child.text===String(item.number));return label && item.height+0.01<label.implicitHeight;});if(small){const label=audit.walk(small).find(child=>child.font && child.text===String(small.number));audit.findings.push({case:entry,problem:"year date text exceeds cell",height:small.height,textHeight:label.implicitHeight});}
  }
  if(entry.app==="Calendar" && entry.view==="Month"){
   const today=audit.walk(window.contentItem).find(item=>item.current===true && item.day!==undefined),label=today?audit.walk(today).find(item=>item.text===String(today.day.getDate())):null,mark=label?label.parent:null;check(label && mark.color,"Actual current date mark");const contrast=audit.ratio(label.color,mark.color);if(contrast<4.5)audit.findings.push({case:entry,problem:"current date contrast",contrast:contrast});
  }
  if(entry.app==="Calendar" && ["Day","Week"].includes(entry.view)){
   const hours=audit.walk(window.contentItem).filter(item=>item.objectName==="calendar-hour-label");check(hours.length===24,"Actual24 hour labels");for(const hour of hours)check(hour.width+0.01>=hour.implicitWidth,"Locale hour label must not elide: "+hour.text+" width "+hour.width+" implicit "+hour.implicitWidth);
   const band=audit.walk(window.contentItem).find(item=>item.objectName==="calendar-all-day-band"),columns=Array.from(band.children).filter(item=>item.day!==undefined);check(columns.length>0,"Actual all-day columns");const scroll=audit.walk(window.contentItem).find(item=>item.objectName==="calendar-time-scroll");check(scroll && scroll.y>=band.y+band.height-1,"Time scroller starts below all-day content");const contentHeight=Math.max(...columns.map(item=>item.implicitHeight));if(band.height+0.01<contentHeight)audit.findings.push({case:entry,problem:"three all-day events overflow band",height:band.height,contentHeight:contentHeight});
   const labels=audit.walk(window.contentItem).filter(item=>item.font && typeof item.text==="string" && item.text.includes("Planning session") && audit.visible(item,window.contentItem));check(labels.length>0,"Actual timed event presentation");for(const item of labels){check(item.font.pixelSize===Math.round(11*entry.scale),"Event text role scales");if(item.height+0.01<item.implicitHeight)audit.findings.push({case:entry,problem:"event text line clipped",height:item.height,textHeight:item.implicitHeight});}
  }
  const paintedLabel=entry.app==="Calendar"?audit.text(window,"Accounts…"):label,paintedAt=paintedLabel.mapToItem(window.contentItem,0,0),paintedText={x:paintedAt.x,y:paintedAt.y,width:paintedLabel.width,height:paintedLabel.height,color:{r:paintedLabel.color.r,g:paintedLabel.color.g,b:paintedLabel.color.b}};
  const eventLabel=entry.app==="Calendar" && ["Day","Week"].includes(entry.view)?audit.walk(window.contentItem).find(item=>item.font && typeof item.text==="string" && item.text.startsWith("Planning session") && audit.visible(item,window.contentItem)):null,eventAt=eventLabel?eventLabel.mapToItem(window.contentItem,0,0):null,paintedEvent=eventLabel?{x:eventAt.x,y:eventAt.y,width:eventLabel.width,height:eventLabel.height,color:{r:eventLabel.color.r,g:eventLabel.color.g,b:eventLabel.color.b}}:null;
  if(entry.app==="Files" && entry.view==="columns"){const columns=audit.walk(window.contentItem).find(item=>item.objectName==="files-column-view"),point=label.mapToItem(columns,0,0),state={entry:entry,visible:columns.visible,width:columns.width,contentWidth:columns.contentWidth,contentX:columns.contentX,currentPath:columns.currentPath,selectionFolder:columns.selectionFolder,label:paintedText};console.log("COLUMN_TYPOGRAPHY_STATE "+JSON.stringify(state));check(point.x>=-1 && point.x+label.width<=columns.width+1,"Switching to columns reveals current-folder label: "+JSON.stringify(state));}
  const capture=audit.paint(window);check(capture,"Actual production content item");audit.painting=true;
  if(!capture.grabToImage(result=>{try{const filename="__OUTPUT__/"+audit.index+".png";check(result.saveToFile(filename),"Painted frame saved");audit.records=audit.records.concat([{entry:entry,font:label.font.pixelSize,resolvedFamily:label.fontInfo.family,primary:{x:at.x,y:at.y,width:label.width,height:label.height},focusable:focusable.length,focusedAndRevealed:true,paintedText:paintedText,paintedEvent:paintedEvent,capture:filename}]);audit.index++;audit.stage=0;audit.painting=false;}catch(error){console.error("OWNED_TYPOGRAPHY_FAILURE "+error);Qt.quit();}},Qt.size(window.width,window.height)))throw Error("Painted capture rejected");
 }catch(error){console.error("OWNED_TYPOGRAPHY_FAILURE "+error);Qt.quit();}}}
}

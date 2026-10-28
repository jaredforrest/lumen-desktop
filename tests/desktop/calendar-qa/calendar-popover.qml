import QtQuick
import Quickshell
import Quickshell.Wayland
import Quickshell.Io
import "source" as Desktop
import "source/CalendarLogic.js" as Logic
ShellRoot {
 id:root
 property date now:new Date(2026,9,14,12,0)
 QtObject {id:notifications
  property bool available:true
  property bool dnd:false
  property string error:""
  property var expandedCards:({})
  property var commands:[]
  property var policies:({"other-app":"banners"})
  property var history:Array.from({length:3},(_,i)=>({id:i+1,app:"Fixture App",desktop:"fixture-app",summary:"Example notification "+i,body:"A synthetic notification. No personal history is read.",timestamp:Date.now(),actions:[{id:"default",label:"Open"}],icon:"",image:""}))
  property var groups:[{key:"fixture-app",name:"Fixture App",rows:history}]
  function command(data){commands=commands.concat([data]);if(data.action==="policy"){const next=Object.assign({},policies);next[data.app]=data.policy;policies=next;}}
  function invoke(row,key){command({action:"invoke",id:row.id,key:key});}
  function seen(){}
  function toggleExpanded(id){const next=Object.assign({},expandedCards);next[id]=!next[id];expandedCards=next;}
 }
 QtObject {id:stub
  property var notifications:notifications
  property date currentDate:root.now
  function openCalendarAt(day,event){full.openAt(day,event);}
  function openSettings(page){}
  function claimPopup(name){}
  function focusOwnedWindow(name){}
 }
 Desktop.CalendarWindow {id:full;shell:stub}
 PanelWindow {visible:!full.visible;WlrLayershell.keyboardFocus:WlrKeyboardFocus.Exclusive;anchors {top:true;bottom:true;left:true;right:true} color:"#798a9c"
  Flickable {id:viewport;x:12;y:12;width:Math.min(344,parent.width-24);height:parent.height-24;contentWidth:width;contentHeight:panel.height;clip:true
   Desktop.NotificationPanel {id:panel;width:viewport.width;shell:stub;today:root.now}
  }
 }
 function find(item,name){if(item.objectName===name)return item;for(const child of item.children || []){const result=find(child,name);if(result)return result;}return null;}
 function card(){return find(panel,"calendar-widget");}
 IpcHandler {target:"audit"
  function status():string{return JSON.stringify(Object.assign(panel.status(),{scale:viewport.scale,viewportHeight:viewport.height,contentHeight:viewport.contentHeight,commands:notifications.commands,policies:notifications.policies,editorVisible:full.visible,gridFocus:find(card(),"calendar-grid").activeFocus}));}
  function check():string {
   if(Logic.iso(Logic.monthShift(new Date(2024,0,31),1))!=="2024-02-29")throw Error("Leap-year month navigation");
   if(Logic.iso(Logic.monthShift(new Date(2026,0,31),1))!=="2026-02-28")throw Error("Month-end clamp");
   if(Logic.offset(new Date(2026,10,1),1)!==6 || Logic.offset(new Date(2026,10,1),7)!==0)throw Error("Locale first weekday");
   if(Logic.tint({color:"#aa44cc"},false)!=="#aa44cc")throw Error("Provider color lost");
   for(const name of ["en_US","en_GB"]){const locale=Qt.locale(name),labels=Logic.weekdayNames(locale,locale.firstDayOfWeek,Locale.ShortFormat);
    if(labels.length!==7 || labels.some(x=>!x) || labels[0]!==locale.standaloneDayName(locale.firstDayOfWeek,Locale.ShortFormat) || labels[6]!==locale.standaloneDayName((locale.firstDayOfWeek+6)%7,Locale.ShortFormat))throw Error("Weekday header shifted for "+name);
   }
   if(Logic.clockDatePattern(Qt.locale("en_US"),Locale.LongFormat)!=="ddd MMM d" || Logic.clockDatePattern(Qt.locale("en_GB"),Locale.LongFormat)!=="ddd d MMM")throw Error("Clock month/day order does not follow locale");
   const c=card();if(c.dayColors(c.selected).length!==2)throw Error("Month-grid calendar colours lost");c.selectDate(new Date(root.now));root.now=new Date(2026,9,15,12,0);
   if(c.iso(c.selected)!=="2026-10-15")throw Error("Today stopped following midnight");
   c.selectDate(new Date(2026,8,1));root.now=new Date(2026,9,16,12,0);
   if(c.iso(c.selected)!=="2026-09-01")throw Error("Midnight stole manual selection");
   c.selectDate(new Date(2026,0,31));c.moveDate(Qt.Key_PageDown);
   if(c.iso(c.selected)!=="2026-02-28")throw Error("Keyboard month navigation");
   c.goToday();const selectedCell=find(c,"calendar-day-"+c.iso(c.selected));
   if(!selectedCell || !selectedCell.Accessible.checked || !selectedCell.Accessible.name.includes("today"))throw Error("Accessible selected/today state missing");
   const grid=find(c,"calendar-grid");grid.forceActiveFocus();
   return "PASS date rollover, manual browsing, leap years, locale offsets, month navigation and provider colors";
  }
  function focusGrid():void {find(card(),"calendar-grid").forceActiveFocus();}
  function preferences(style:string,first:bool,show:bool):void {panel.setPreference("style",style);panel.setPreference("calendarFirst",first);panel.setPreference("calendarVisible",show);}
  function today():void {card().goToday();}
  function collapse():void {const top=find(panel,"notification-card");if(top && !top.expandStack)panel.children.filter(x=>x.expandedGroups!==undefined).forEach(x=>x.expandedGroups=({}));}
  function editWidgets():void {panel.editingWidgets=true;}
  function editor():void {full.open();full.create(new Date(2026,9,14));}
  function editorCheck():string {
   const editor=full.calendarState;const field=find(editor,"date-time-starts");
   if(!field)throw Error("Date/time control missing");
   find(field,"date-time-starts-year").value=2028;find(field,"date-time-starts-month").currentIndex=1;find(field,"date-time-starts-day").value=29;
   find(field,"date-time-starts-hour").value=23;find(field,"date-time-starts-minute").value=45;field.commit();
   if(field.text!=="2028-02-29T23:45")throw Error("Date/time controls did not serialize leap-day time: "+field.text);
   field.allDay=true;field.commit();if(field.text!=="2028-02-29")throw Error("All-day serialization");
   return "PASS accessible states and date/time controls preserve leap dates, time and all-day values";
  }
  function notificationPolicy():string {
   const top=find(panel,"notification-card");top.deliveryMenu.itemAt(1).triggered();
   if(notifications.policies["fixture-app"]!=="silent" || notifications.policies["other-app"]!=="banners")throw Error("Per-app policy changed the wrong application");return "PASS per-app notification preference isolation";
  }
  function notificationCheck():string {
   const top=find(panel,"notification-card");const dismiss=find(top,"notification-dismiss");if(dismiss.visible)throw Error("Dismiss control should stay hidden at rest");top.forceActiveFocus();if(!dismiss.visible)throw Error("Keyboard focus must reveal dismissal");if(!top || !top.expandStack)throw Error("Missing collapsed stack");
   top.activate();if(notifications.commands.some(x=>x.action==="invoke"))throw Error("Stack expansion opened app");
   return "PASS stack expansion is distinct from opening notification";
  }
  function notificationOpen():string {const top=find(panel,"notification-card");if(top.expandStack)throw Error("Stack remained collapsed");top.activate();if(!notifications.commands.some(x=>x.action==="invoke" && x.id===1))throw Error("Expanded alert did not open");return "PASS expanded notification opens selected item";}
 }
}

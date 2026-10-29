import QtQuick
import QtQuick.Window
import Quickshell
import Quickshell.Io
import "assets/shell" as D
ShellRoot {
 id:root
 property date currentDate:new Date(2026,9,31,12)
 property var actions:[]
 property var widgets:widgetService
 QtObject {id:widgetService;property var preferences:({reduceTransparency:false});function positionWidget(id,output,x,y){}function edit(){}}
 function openCalendarAt(day,event){actions=actions.concat([{kind:event?"event":"related",day:Qt.formatDate(day,"yyyy-MM-dd"),id:event?event.id:null,title:event?event.title:null,provider:event?event.provider:null,calendar:event?event.calendar:null}]);}
 function openSettings(page){actions=actions.concat([{kind:"settings",page:page}]);}
 function showMenu(name,output){actions=actions.concat([{kind:"menu",name:name}]);}
 FloatingWindow {id:window;title:"Calendar widget fixture";visible:true;implicitWidth:500;implicitHeight:760;color:D.Theme.colors.background
  D.DesktopWidgetItem {id:widget;shell:root;descriptor:({id:"fixture-calendar",type:"calendar",size:"large",style:"month",positions:({}),output:"*"});outputName:"HEADLESS-1";widgetIndex:0;availableWidth:window.width;availableHeight:window.height;active:true}
 }
 function find(name,item){if(!item)return null;if(item.objectName===name)return item;for(const child of item.children || []){const result=find(name,child);if(result)return result;}return null;}
 function card(){return find("widget-calendar-content",widget);}
 IpcHandler {target:"filesAudit"
  function eventName():string {return find("calendar-event-"+card().events[1].id,widget).Accessible.name;}
  function wallTime():string {return card().timeLabel({date:"2026-03-29",startTime:"01:30",allDay:false});}
  function refresh():void {card().refresh();}
  function clock(value:string):void {root.currentDate=new Date(value+"T12:00:00");}
  function select(value:string):void {card().selectDate(new Date(value+"T12:00:00"));}
  function today():void {card().goToday();}
  function pause(value:bool):void {card().active=!value;}
  function focusGrid():void {find("calendar-grid",widget).forceActiveFocus();}
  function focusRelated():void {find("calendar-open-related",widget).forceActiveFocus();}
  function focusEvent():void {find("calendar-event-"+card().events[1].id,widget).forceActiveFocus();}
  function state():string {const c=card();const related=find("calendar-open-related",widget);const at=related.mapToItem(window.contentItem,0,0);return JSON.stringify({ready:!c.busy && !c.refreshing,selected:c.iso(c.selected),month:c.iso(c.month),following:c.followingToday,events:c.events,loaded:c.loaded,error:c.error,actions:root.actions,related:{y:at.y,height:related.height},focus:window.contentItem.Window.window.activeFocusItem?window.contentItem.Window.window.activeFocusItem.objectName:""});}
  function geometry(name:string):string {const item=find(name,widget);if(!item)return "null";let ancestor=item.parent;while(ancestor){if(ancestor.contentY!==undefined && ancestor.contentItem){const pos=item.mapToItem(ancestor.contentItem,0,0);ancestor.contentY=Math.max(0,Math.min(ancestor.contentHeight-ancestor.height,pos.y-80));break;}ancestor=ancestor.parent;}const at=item.mapToItem(window.contentItem,0,0);return JSON.stringify({x:at.x,y:at.y,width:item.width,height:item.height});}
 }
}
import QtQuick
import QtQuick.Controls
import QtQuick.Window
import Quickshell
import Quickshell.Io
import "assets/shell" as D
ShellRoot {
 id:audit
 property var palettes:__PALETTES__
 property string wanted:""
 QtObject {id:stub;property date currentDate:new Date(2026,8,28);property int accounts:0;function claimPopup(name){}function focusOwnedWindow(name){}function openSettings(name){accounts++;}}
 D.CalendarWindow {id:owner;shell:stub}
 function walk(item,seen){if(!item || seen.includes(item))return [];seen.push(item);let result=[item];for(const child of Array.from(item.children || []).concat(Array.from(item.resources || [])))result=result.concat(walk(child,seen));if(item.contentItem)result=result.concat(walk(item.contentItem,seen));return result;}
 function shown(item){while(item && item!==owner.contentItem){if(!item.visible)return false;item=item.parent;}return item===owner.contentItem;}
 function rect(item){const p=item.mapToItem(owner.contentItem,0,0);return {x:p.x,y:p.y,width:item.width,height:item.height};}
 function focusedName(){for(let item=owner.contentItem.Window.window.activeFocusItem;item;item=item.parent){if(typeof item.mapToItem!=="function")continue;if(item.Accessible.name)return item.Accessible.name;if(item.clicked && item.text)return item.text;}return "";}
 IpcHandler {target:"calendarDetails"
  function ready():bool{return true;}
  function open(mode:string):void {owner.visible=false;owner.inspector=false;owner.calendarState.editing=false;D.Theme.font=palettes.font;D.Theme.colors=palettes[mode];D.Theme.preferences={textScale:1.5,reduceMotion:true,reduceTransparency:true};owner.implicitWidth=480;owner.implicitHeight=320;owner.visible=true;Qt.callLater(()=>owner.contentItem.Window.window.requestActivate());}
  function details():void {owner.eventDetails(owner.calendarState.events[0],new Date(2026,8,28));owner.calendarState.optionsOpen=true;}
  function focus(label:string):void {audit.wanted=label;const target=walk(owner.contentItem,[]).find(item=>typeof item.mapToItem==="function" && shown(item) && ((item.clicked && item.text===label) || item.Accessible.name===label));if(!target)throw Error("Missing actual control "+label);target.forceActiveFocus();}
  function state():string {const backing=owner.contentItem.Window.window;if(!backing)return JSON.stringify({active:false});const items=walk(owner.contentItem,[]).filter(item=>typeof item.mapToItem==="function" && shown(item)),target=wanted?items.find(item=>item.clicked && item.text===wanted && item.contentItem && item.contentItem.font):null;return JSON.stringify({active:backing.active,visible:owner.visible,loaded:owner.calendarState.loaded,busy:owner.calendarState.busy,events:owner.calendarState.events.length,editing:owner.calendarState.editing,inspector:owner.inspector,detail:owner.calendarState.detail?owner.calendarState.detail.id:null,focus:focusedName(),accounts:stub.accounts,width:owner.width,height:owner.height,target:target?{rect:rect(target),textRect:rect(target.contentItem),color:String(target.contentItem.color),pixelSize:target.contentItem.font.pixelSize}:null});}
 }
}

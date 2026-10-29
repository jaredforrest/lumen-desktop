import QtQuick
import QtQuick.Controls
import QtQuick.Window
import Quickshell
import Quickshell.Io
import "assets/shell" as D
ShellRoot {
 id:audit
 property var owner:window
 property string kind:""
 property string wanted:""
 FloatingWindow {id:window;title:"Shared Controls";visible:false;implicitWidth:300;implicitHeight:400
  Rectangle {id:surface;anchors.fill:parent;color:D.Theme.colors.background
   Column {x:16;y:16;width:parent.width-32;spacing:12
    D.AudioDeviceControl {id:audio;visible:audit.kind==="Audio";width:parent.width;node:({id:1,nickname:"Private playback device",audio:{volume:.5,muted:false}})}
    D.BrightnessControl {id:brightness;visible:audit.kind==="Brightness";width:parent.width}
    D.DateTimeField {id:date;visible:audit.kind==="Date";width:parent.width;label:"Starts";text:"2026-09-28T23:59"}
   }
   Shortcut {sequence:"Escape";onActivated:window.visible=false}
  }
 }
 property var returnActions:[]
 property int returnPointerPresses:0
 FloatingWindow {id:returns;title:"Return eligibility";visible:false;implicitWidth:420;implicitHeight:190
  Rectangle {anchors.fill:parent;color:D.Theme.colors.background
   TextField {id:returnInput;onPressed:++audit.returnPointerPresses;x:16;y:16;width:parent.width-32;Accessible.name:"Default input";font.family:D.Theme.font;font.pixelSize:D.Theme.textRole("body")}
   Row {x:16;y:80;spacing:8
    D.DialogButton {id:returnCancel;text:"Cancel";onClicked:audit.returnActions=audit.returnActions.concat("cancel")}
    D.DialogButton {id:returnOther;text:"Other";onClicked:audit.returnActions=audit.returnActions.concat("other")}
    D.DialogButton {id:returnPrimary;text:"Proceed";primary:true;onClicked:audit.returnActions=audit.returnActions.concat("primary")}
   }
  }
 }
 QtObject {id:stub;property date currentDate:new Date(2026,8,28);function claimPopup(name){}function focusOwnedWindow(name){}function openSettings(name){}}
 D.CalendarWindow {id:calendar;shell:stub}
 function walk(item,seen){if(!item || seen.includes(item))return [];seen.push(item);let result=[item];for(const child of Array.from(item.children || []).concat(Array.from(item.resources || [])))result=result.concat(walk(child,seen));if(item.contentItem)result=result.concat(walk(item.contentItem,seen));return result;}
 function shown(item){while(item && item!==owner.contentItem){if(!item.visible)return false;item=item.parent;}return item===owner.contentItem;}
 function name(item){while(item){if(typeof item.mapToItem==="function" && item.Accessible.name)return item.Accessible.name;item=item.parent;}return "";}
 function rect(item){const p=item.mapToItem(owner.contentItem,0,0);return {x:p.x,y:p.y,width:item.width,height:item.height};}
 IpcHandler {target:"sharedTypography"
  function ready():bool {return true;}
  function open(value:string):void {returns.visible=false;window.visible=false;calendar.visible=false;audit.kind=value;audit.owner=value==="CalendarEditor"?calendar:window;owner.implicitWidth=value==="CalendarEditor"?480:300;owner.implicitHeight=value==="CalendarEditor"?320:400;owner.visible=true;Qt.callLater(()=>owner.contentItem.Window.window.requestActivate());}
  function returnCase(mode:string):void {returns.visible=false;window.visible=false;calendar.visible=false;audit.kind="ReturnSemantics";audit.owner=returns;returnInput.text="";audit.returnActions=[];audit.returnPointerPresses=0;returnPrimary.enabled=mode!=="disabled";returnPrimary.visible=mode!=="hidden";returns.visible=true;Qt.callLater(()=>returns.contentItem.Window.window.requestActivate());}
  function focusChild(value:string):void {const button=value==="Cancel"?returnCancel:returnPrimary;audit.wanted=value;button.contentItem.forceActiveFocus();}
  function edit():void {calendar.create(calendar.selected);}
  function focus(value:string):void {audit.wanted=value;const item=walk(owner.contentItem,[]).find(item=>typeof item.mapToItem==="function" && item.Accessible.name===value && shown(item));if(!item)throw Error("Visible control missing "+value);item.forceActiveFocus();}
  function state():string {const backing=owner.contentItem.Window.window;if(!backing)return JSON.stringify({visible:owner.visible,active:false,width:owner.width,height:owner.height,scale:D.Theme.textScale});const items=walk(owner.contentItem,[]),focus=owner.contentItem.Window.window.activeFocusItem,target=items.find(item=>typeof item.mapToItem==="function" && item.Accessible.name===audit.wanted && shown(item)),tip=walk(brightness,[]).find(item=>item.contentItem && item.padding===7 && typeof item.open==="function"),slider=walk(brightness,[]).find(item=>typeof item.mapToItem==="function" && item.Accessible.name==="Display brightness");let ancestry=[];for(let item=focus;item;item=item.parent){if(item===returnCancel)ancestry.push("cancel");if(item===returnOther)ancestry.push("other");if(item===returnPrimary)ancestry.push("primary");}return JSON.stringify({returnPointerPresses:audit.returnPointerPresses,returnActions:audit.returnActions,input:returnInput.text,focusAncestry:ancestry,visible:owner.visible,active:owner.contentItem.Window.window.active,focus:name(focus),target:target?rect(target):null,targetText:target && target.contentItem && target.contentItem.font?{rect:rect(target.contentItem),color:String(target.contentItem.color)}:null,width:owner.width,height:owner.height,scale:D.Theme.textScale,date:date.text,loaded:calendar.calendarState.loaded,busy:calendar.calendarState.busy,editing:calendar.calendarState.editing,tooltip:tip?{opened:tip.opened,text:tip.text,color:String(tip.contentItem.color),pixelSize:tip.contentItem.font.pixelSize,rect:tip.opened?rect(tip.contentItem):null}:null,slider:audit.kind==="Brightness" && slider?rect(slider):null});}
 }
}

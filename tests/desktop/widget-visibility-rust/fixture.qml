import QtQuick
import QtQuick.Window
import Quickshell
import Quickshell.Io
import Quickshell.Wayland
import "assets/shell" as D
ShellRoot {
 id:root
 property date currentDate:new Date(2026,10,1,13,5)
 property var actions:[]
 property int backgroundClicks:0
 property string bridgeError:""
 property int bridgeExit:-999
 property alias widgets:widgetState
 property alias exportedMenu:menu
 property var widgetSurfaces:({})
 D.WidgetState {id:widgetState}
 QtObject {id:menu;property var state:({windowId:0});property var latestState:state}
 function openCalendarAt(day,event){actions=actions.concat([{kind:event?"event":"related",id:event?event.id:null,day:Qt.formatDate(day,"yyyy-MM-dd")}]);}
 function showMenu(name,output){actions=actions.concat([{kind:"menu",output:output}]);}
 function openSettings(page){}
 Process {id:bridge;running:true;stdinEnabled:true;command:[Quickshell.env("LUMEN_MENU_BRIDGE")]
  stderr:StdioCollector {onStreamFinished:root.bridgeError=text}
  onExited:(code,status)=>root.bridgeExit=code
  stdout:SplitParser {onRead:line=>{try{const value=JSON.parse(line);menu.state=value;menu.latestState=value;}catch(error){console.warn("Invalid native menu state",line);}}}
 }
 Variants {model:Quickshell.screens
  Scope {required property var modelData
   PanelWindow {screen:modelData;anchors {top:true;left:true;right:true;bottom:true}color:"#256048";exclusiveZone:0;WlrLayershell.layer:WlrLayer.Background;WlrLayershell.keyboardFocus:WlrKeyboardFocus.OnDemand
    MouseArea {anchors.fill:parent;onClicked:root.backgroundClicks++}
   }
   D.DesktopWidgets {screen:modelData;outputName:modelData.name;shell:root}
  }
 }
 function find(name,item){if(!item)return null;if(item.objectName===name)return item;for(const child of item.children || []){const result=find(name,child);if(result)return result;}return null;}
 function item(id,output){const surface=root.widgetSurfaces[output];return surface?surface.findWidget(id):null;}
 function rows(){let result=[];for(const name of Object.keys(root.widgetSurfaces)){const surface=root.widgetSurfaces[name];for(const row of surface.status()){const widget=item(row.id,name);const material=find("widget-material",widget);const calendar=widget.descriptor.type==="calendar"?find("widget-calendar-content",widget):null;const related=find("calendar-open-related",widget);const at=related?related.mapToItem(surface.contentItem,0,0):null;result.push(Object.assign({},row,{output:name,screenX:surface.screen.x,screenY:surface.screen.y,visible:surface.visible,monochrome:widget.monochrome,opacity:widget.opacity,opaque:widget.reduceTransparency,alpha:material.color.a,selected:calendar?calendar.iso(calendar.selected):null,loaded:calendar?calendar.loaded:true,related:at?{x:at.x,y:at.y,width:related.width,height:related.height}:null}));}}return result;}
 IpcHandler {target:"filesAudit"
  function dayStyle():string {const cell=find("calendar-day-2026-11-01",item("calendar-medium","HEADLESS-1"));return JSON.stringify({foreground:String(cell.contentItem.color),background:String(cell.background.children[0].color),font:cell.contentItem.font.pixelSize});}
  function preference(key:string,value:string):void {widgetState.setPreference(key,JSON.parse(value));}
  function update(id:string,patch:string):void {widgetState.updateWidget(id,JSON.parse(patch));}
  function focusRelated(id:string,output:string):void {find("calendar-open-related",item(id,output)).forceActiveFocus();}
  function focusClock(id:string,output:string):void {find("widget-clock-content",item(id,output)).forceActiveFocus();}
  function state():string {return JSON.stringify({rows:rows(),bridgeRunning:bridge.running,bridgeError:root.bridgeError,bridgeExit:root.bridgeExit,windowId:menu.latestState.windowId,app:menu.latestState.appName,actions:root.actions,backgroundClicks:root.backgroundClicks,error:widgetState.error});}
  function geometry(id:string,output:string,name:string):string {const surface=root.widgetSurfaces[output];const widget=item(id,output);const target=name?find(name,widget):widget;if(!target)return "null";let ancestor=target.parent;while(name && ancestor){if(ancestor.contentY!==undefined && ancestor.contentItem){const pos=target.mapToItem(ancestor.contentItem,0,0);ancestor.contentY=Math.max(0,Math.min(ancestor.contentHeight-ancestor.height,pos.y-70));}ancestor=ancestor.parent;}const at=target.mapToItem(surface.contentItem,0,0);return JSON.stringify({x:at.x+surface.screen.x,y:at.y+surface.screen.y,width:target.width,height:target.height});}
 }
}
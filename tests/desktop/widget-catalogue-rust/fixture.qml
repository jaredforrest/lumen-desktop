import QtQuick
import QtQuick.Controls
import QtQuick.Window
import Quickshell
import Quickshell.Io
import Quickshell.Wayland
import "assets/shell" as D
ShellRoot {
 id:fixture
 property int activations:0
 property int backgroundClicks:0
 property var captured:null
 property var capturedEditor:null
 property alias widgetService:widgets
 D.WidgetState {id:widgets}
 QtObject {id:menu;property var state:({windowId:0})}
 QtObject {id:stub;property var widgets:fixture.widgetService;property var exportedMenu:menu;property var widgetSurfaces:({});property date currentDate:new Date(2026,9,19,13,5);function showMenu(kind,output){fixture.activations++;}function openSettings(page){}function openCalendarAt(day,event){fixture.activations++;}}
 Variants {model:Quickshell.screens
  Scope {required property var modelData
   PanelWindow {screen:modelData;anchors {top:true;bottom:true;left:true;right:true} color:"#303030";exclusiveZone:0;WlrLayershell.layer:WlrLayer.Background;MouseArea {anchors.fill:parent;onClicked:fixture.backgroundClicks++}}
   D.DesktopWidgets {screen:modelData;outputName:modelData.name;shell:stub}
  }
 }
 FloatingWindow {id:window;title:"Widget Catalogue";visible:true;implicitWidth:420;implicitHeight:680
  Rectangle {id:body;anchors.fill:parent;color:D.Theme.colors.background
   D.FocusFlickable {id:scroll;anchors.fill:parent;anchors.margins:20;contentWidth:width;contentHeight:editor.implicitHeight;clip:true;D.DesktopWidgetEditor {id:editor;width:scroll.width;service:widgets}}
  }
 }
 function find(item,name){if(item.objectName===name)return item;for(const child of item.children || []){const result=find(child,name);if(result)return result;}return null;}
 IpcHandler {target:"widgetAudit"
  function state():string{return JSON.stringify({preferences:widgets.preferences,error:widgets.error,outputs:Object.keys(stub.widgetSurfaces).filter(name=>stub.widgetSurfaces[name]).map(name=>({name:name,items:stub.widgetSurfaces[name].status()})),activations:fixture.activations,background:fixture.backgroundClicks,scrollY:scroll.contentY,focused:window.activeFocusItem?window.activeFocusItem.objectName:"",sameEditor:!fixture.capturedEditor || widgets.desktopItems.length>0 && find(body,"widget-size-"+widgets.desktopItems[0].id)===fixture.capturedEditor,same:!fixture.captured || widgets.desktopItems.length>0 && stub.widgetSurfaces["HEADLESS-1"].findWidget(widgets.desktopItems[0].id)===fixture.captured});}
  function point(name:string):string{const item=find(body,name);if(!item)throw Error("Missing "+name);const p=item.mapToItem(body,item.width/2,item.height/2);return JSON.stringify({x:p.x,y:p.y});}
  function widgetPoint(name:string):string{const card=stub.widgetSurfaces["HEADLESS-1"].findWidget(widgets.desktopItems[0].id);const target=find(card,name);if(!target)throw Error("Missing widget control "+name);const p=target.mapToItem(card,target.width/2,target.height/2);return JSON.stringify({x:card.x+p.x,y:card.y+p.y});}
  function capture():void{fixture.capturedEditor=find(body,"widget-size-"+widgets.desktopItems[0].id);fixture.captured=stub.widgetSurfaces["HEADLESS-1"].findWidget(widgets.desktopItems[0].id);}
 }
}
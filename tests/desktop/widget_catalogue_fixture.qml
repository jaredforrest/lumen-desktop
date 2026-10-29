import QtQuick
import Quickshell
import "LUMEN_ASSETS" as D
ShellRoot {
 id:fixture
 property var restored:null
 Component {id:restorer;D.WidgetState {}}
 D.WidgetState {id:state}
 QtObject {id:stub;property var widgets:state;property date currentDate:new Date(2026,9,19,13,5);function showMenu(kind,output){}function openSettings(page){}function openCalendarAt(day,event){}}
 D.DesktopWidgetItem {id:card;shell:stub;descriptor:state.desktopItems[0] || {id:"fallback",type:"clock",size:"small",positions:{},hourCycle:"locale"};outputName:"test";widgetIndex:0;availableWidth:480;availableHeight:320;active:false}
 D.DesktopWidgetEditor {service:state;width:300}
 property string savedId:""
 Timer {interval:150;running:true;onTriggered:{try {
  state.addWidget("clock");savedId=state.desktopItems[0].id;
  if(state.desktopItems.length!==1 || !state.preferences.desktopVisible)throw Error("Legacy hidden migration");
  state.addWidget("calendar");state.updateWidget(savedId,{size:"large",hourCycle:"24",output:"disconnected"});state.positionWidget(savedId,"test",.25,.5);
  if(state.desktopItems.length!==2 || state.desktopItems[0].size!=="large")throw Error("Catalogue mutation");
  card.beginMove(null);card.moveTo(400,400);card.cancelMove();
  if(card.preview || state.desktopItems[0].positions.test.x!==.25)throw Error("Cancel persisted preview");
  state.updateWidget("stale-id",{size:"small"});
  if(state.desktopItems[0].size!=="large")throw Error("Stale target");
 }catch(error){console.error("WIDGET_FAIL "+error);Qt.quit();}}}
 Timer {interval:600;running:true;onTriggered:fixture.restored=restorer.createObject(fixture)}
 Timer {interval:950;running:true;onTriggered:{if(state.error)console.error("WIDGET_FAIL "+state.error);else if(!fixture.restored || fixture.restored.desktopItems.length!==2 || fixture.restored.desktopItems[0].positions.test.x!==.25)console.error("WIDGET_FAIL Restart state");else console.log("WIDGET_PASS");Qt.quit();}}
}
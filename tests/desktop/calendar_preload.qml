import QtQuick
import Quickshell
import "__DESKTOP__" as D
ShellRoot {
 D.CalendarCard {id:card;today:new Date();visible:false;active:false;width:344;compact:true}
 Timer {interval:50;repeat:true;running:true;onTriggered:{
  if(card.error){console.error("PRELOAD_FAILED",card.error);Qt.exit(1);return;}
  if(!card.loaded || card.busy)return;stop();
  card.visible=true;card.active=true;
  if(!card.loaded || card.busy){console.error("OPEN_RELOADED");Qt.exit(1);return;}
  D.CalendarHealth.report("fixture failure");D.CalendarHealth.report("fixture failure");
  finish.start();
 }}
 Timer {id:finish;interval:500;onTriggered:{console.log("CALENDAR_PRELOAD_PASS");Qt.quit();}}
 Timer {interval:8000;running:true;onTriggered:Qt.exit(2)}
}

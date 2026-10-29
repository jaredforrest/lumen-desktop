import QtQuick
import QtQuick.Controls
import QtTest
import Quickshell
import "baseline-assets/shell" as D
ShellRoot {
 Window {id:view;visible:true;width:320;height:240
  D.BatteryMenuButton {id:button;x:100;y:80;power:({available:true,present:true,percentage:70,state:"discharging",secondsRemaining:3600})}
  TestCase {id:input;when:false;name:"Committed battery tooltip negative"}
  Timer {interval:150;running:true;onTriggered:{try{input.mouseMove(button,20,10);input.wait(120);if(button.hovered || button.ToolTip.visible || button.hoverEnabled || button.ToolTip.delay!==0 || button.ToolTip.timeout!==-1)throw Error("Committed missing-hover/unbounded baseline not reproduced "+JSON.stringify({hovered:button.hovered,visible:button.ToolTip.visible,delay:button.ToolTip.delay,timeout:button.ToolTip.timeout,enabled:button.enabled,hoverEnabled:button.hoverEnabled}));console.log("TOOLTIP_BASELINE_REPRODUCED "+JSON.stringify({delay:button.ToolTip.delay,timeout:button.ToolTip.timeout,hovered:button.hovered,hoverEnabled:button.hoverEnabled}));Qt.quit();}catch(error){console.error("TOOLTIP_BASELINE_FAIL "+error);Qt.exit(1);}}}
 }
}

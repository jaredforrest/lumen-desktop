import QtQuick
import QtQuick.Controls
import QtQuick.Window
import QtTest
import Quickshell
import "assets/shell" as D
ShellRoot {
 id:test
 property int checks:0
 property bool imageSaved:false
 property bool ownerTipDestroyed:false
 function settle(predicate,message,limit){for(let elapsed=0;elapsed<limit;elapsed+=20){if(predicate()){check(true,message);return;}input.wait(20);}check(false,message);}
 function check(value,message){if(!value)throw Error(message);checks++;}
 Window {
  id:view;visible:true;width:320;height:240;color:D.Theme.colors.background
  D.BatteryMenuButton {id:battery;x:12;y:12;power:({available:true,present:true,percentage:70,state:"discharging",secondsRemaining:3600})}
  AbstractButton {id:button;x:260;y:200;width:48;height:30;hoverEnabled:true;activeFocusOnTab:true;Accessible.name:"Named control";Accessible.description:"Essential explanation stays available";background:Rectangle {color:D.Theme.colors.surface} contentItem:Text {text:"Help";color:D.Theme.colors.text}}
  D.OwnedToolTip {id:tip;target:button;text:"A deliberately long supplementary explanation that wraps at enlarged text size and remains inside the owning window near its lower right edge."}
  Popup {id:replacement;x:30;y:30;width:180;height:100;focus:true;modal:true;closePolicy:Popup.CloseOnEscape;contentItem:Text {text:"Replacement overlay"}}
  Component {id:disposable;AbstractButton {id:disposableButton;x:20;y:160;width:60;height:28;hoverEnabled:true;D.OwnedToolTip {Component.onDestruction:test.ownerTipDestroyed=true;objectName:"disposable-tip";target:disposableButton;text:"Disposable owner"}}}
  TestCase {id:input;name:"Tooltip synthetic window input";when:false}
  Timer {interval:150;running:true;repeat:false;onTriggered:{try{
   input.mouseMove(view.contentItem,5,5);input.wait(80);test.check(!tip.visible,"No tip without a target route");
   input.mouseMove(button,20,10);input.wait(120);test.check(!tip.visible,"Hover must wait for bounded delay");
   input.wait(700);test.check(tip.opened,"Real hover opens supplementary help");
   const point=tip.contentItem.mapToItem(view.contentItem,0,0);test.check(point.x>=0 && point.y>=0 && point.x+tip.contentItem.width<=view.width && point.y+tip.contentItem.height<=view.height,"Edge placement bounded inside owner");
   test.check(tip.contentItem.font.pixelSize===D.Theme.textRole("metadata") && tip.contentItem.lineCount>1,"Metadata typography scales and wraps");
   view.contentItem.grabToImage(result=>{test.imageSaved=result.saveToFile("FIXTURE_IMAGE");});input.wait(100);test.check(test.imageSaved,"Actual painted frame captured");
   input.mouseMove(view.contentItem,5,5);input.wait(80);test.check(!tip.visible,"Leave dismisses immediately");
   button.forceActiveFocus();input.keyClick(Qt.Key_F1);input.wait(80);test.check(tip.visible,"Focused F1 help bypasses delay");
   input.keyClick(Qt.Key_Escape);input.wait(80);test.check(!tip.visible,"Escape dismisses without activating owner");
   input.keyClick(Qt.Key_F1);input.wait(80);test.settle(()=>tip.lifecycle.retired && !tip.lifecycle.shown,"Timeout retires lifecycle",4300);test.settle(()=>!tip.visible,"Retired tooltip finishes exit transition",600);
   input.keyClick(Qt.Key_F1);input.wait(80);test.check(tip.visible,"Fresh help request after timeout");button.enabled=false;input.wait(80);test.check(!tip.visible,"Disable dismisses");button.enabled=true;
   input.mouseMove(button,20,10);input.wait(800);test.check(tip.visible,"Reenter opens");input.mousePress(button,20,10);input.wait(80);test.check(!tip.visible,"Press retires hover help");input.mouseRelease(button,20,10);
   button.forceActiveFocus();input.keyClick(Qt.Key_F1);input.wait(80);replacement.open();input.wait(80);test.check(!tip.visible,"Replacement overlay retires help");replacement.close();
   input.mouseMove(view.contentItem,5,5);button.focus=false;const owner=disposable.createObject(view.contentItem);input.mouseMove(owner,20,10);input.wait(800);const ownedTip=Array.from(owner.children).concat(Array.from(owner.resources)).find(child=>child.objectName==="disposable-tip");test.check(ownedTip && ownedTip.visible,"Dynamic owner has real hovered tooltip");owner.destroy();test.settle(()=>test.ownerTipDestroyed && !D.TooltipCoordinator.current && !tip.visible,"Actual owner destruction retires coordinator and popup",1000);
   input.mouseMove(battery,20,10);const batteryTip=Array.from(battery.resources).find(child=>child.objectName==="owned-tooltip");test.settle(()=>batteryTip && batteryTip.visible,"Actual battery hover route",1200);input.mouseMove(view.contentItem,5,5);test.settle(()=>!batteryTip.visible,"Actual battery leave",600);battery.forceActiveFocus();input.keyClick(Qt.Key_F1);test.settle(()=>batteryTip.visible,"Actual battery focused F1 route",400);input.keyClick(Qt.Key_Escape);test.settle(()=>!batteryTip.visible,"Actual battery Escape",600);test.check(battery.Accessible.name.includes("Battery") && battery.Accessible.description.includes("remaining"),"Battery essential accessible status independent");
   test.check(button.Accessible.name==="Named control" && button.Accessible.description.includes("Essential"),"Essential accessible explanation independent of tooltip visibility");
   console.log("TOOLTIP_PASS "+JSON.stringify({checks:test.checks,scale:D.Theme.textScale,width:view.width,height:view.height}));Qt.quit();
  }catch(error){console.error("TOOLTIP_FAIL "+error);Qt.exit(1);}}}
 }
}

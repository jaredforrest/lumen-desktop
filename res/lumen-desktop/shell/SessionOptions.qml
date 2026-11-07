import QtQuick
import QtQuick.Controls
import QtQuick.Window
import Quickshell
import Quickshell.Io
FloatingWindow {
 id:root;title:"Session Options";visible:false;color:"transparent"
 implicitWidth:420
 implicitHeight:confirmationContents.height+(Theme.nativeWindowFrame?18:62)+20
 minimumSize:Qt.size(implicitWidth,Math.ceil(implicitHeight))
 maximumSize:minimumSize
 property string action:""
 readonly property var labels:({suspend:"Sleep",reboot:"Restart",poweroff:"Shut Down",logout:"Log Out"})
 readonly property var commands:({suspend:["systemctl","suspend"],reboot:["systemctl","reboot"],poweroff:["systemctl","poweroff"],logout:["swaymsg","exit"]})
 property string error:""
 property bool executeActions:true
 // Kept as a consumer-compatible property; normal session actions do not
 // require every open application to implement the native Quit protocol.
 property var lifecycle:null
 signal confirmed(string action)
 function dismiss(){if(worker.running)return;visible=false;}
 function show(value){if(!labels[value] || worker.running)return;action=value;error="";visible=true;Qt.callLater(()=>{cancel.forceActiveFocus();if(body.Window.window)body.Window.window.requestActivate();});}
 function confirm(){
  if(!visible || !commands[action] || worker.running)return;
  confirmed(action);
  if(!executeActions){visible=false;return;}
  error="";worker.command=commands[action];worker.running=true;
 }
 Rectangle {id:body;Accessible.role:Accessible.Dialog;Accessible.name:(root.labels[root.action] || "Session")+" confirmation";Accessible.description:"Confirm or cancel this session action.";anchors.fill:parent;color:Theme.colors.background;radius:Theme.nativeWindowFrame?0:12
  MouseArea {width:parent.width;height:48;onPressed:body.Window.window.startSystemMove()}
  WindowControls {windowActive:body.Window.window?body.Window.window.active:false;windowName:"Session Options";canMinimize:false;canFullscreen:false;onTriggered:root.dismiss()}
  Column {id:confirmationContents;x:24;y:Theme.nativeWindowFrame?18:62;width:parent.width-48;spacing:14
   PanelLabel {width:parent.width;text:(root.labels[root.action] || "Session")+"?";font.pixelSize:Theme.textSize(20);font.weight:Font.DemiBold}
   PanelLabel {width:parent.width;wrapMode:Text.WordWrap;text:root.action==="suspend"?"Your session will be available when this computer wakes.":"Save your work before continuing. Your open applications will close."}
   StatusMessage {width:parent.width;text:root.error;severity:"error"}
   Row {anchors.right:parent.right;spacing:10
    DialogButton {id:cancel;text:"Cancel";enabled:!worker.running;onClicked:root.dismiss()}
    DialogButton {text:root.labels[root.action] || "Continue";primary:root.action==="suspend";defaultAction:false;destructive:root.action!=="suspend";enabled:!worker.running;onClicked:root.confirm()}
   }
  }
  Shortcut {sequence:"Escape";onActivated:root.dismiss()}
 }
 Process {id:worker
  stderr:StdioCollector {id:failureOutput;onStreamFinished:if(text.trim())root.error=text.trim()}
  onExited:(code,status)=>{if(code===0)root.visible=false;else root.error=failureOutput.text.trim() || "The system did not accept the request."}
 }
}

import QtQuick
import QtQuick.Controls
import QtQuick.Window
import Quickshell
import Quickshell.Io
FloatingWindow {
 id:root;title:"Session Options";visible:false;color:"transparent"
 implicitWidth:420;implicitHeight:240;minimumSize:Qt.size(320,220)
 property string action:""
 readonly property var labels:({suspend:"Sleep",reboot:"Restart",poweroff:"Shut Down",logout:"Log Out"})
 readonly property var commands:({suspend:["systemctl","suspend"],reboot:["systemctl","reboot"],poweroff:["systemctl","poweroff"],logout:["swaymsg","exit"]})
 property string error:""
 signal confirmed(string action)
 property bool executeActions:true
 function show(value){if(!labels[value])return;action=value;error="";visible=true;Qt.callLater(()=>{cancel.forceActiveFocus();if(body.Window.window)body.Window.window.requestActivate();});}
 function confirm(){if(!commands[action] || worker.running)return;confirmed(action);if(!executeActions){visible=false;return;}worker.command=commands[action];worker.running=true;}
 Rectangle {id:body;anchors.fill:parent;color:Theme.colors.background;radius:12
  MouseArea {width:parent.width;height:48;onPressed:body.Window.window.startSystemMove()}
  WindowControls {windowActive:body.Window.window?body.Window.window.active:false;windowName:"Session Options";canMinimize:false;canFullscreen:false;onTriggered:root.visible=false}
  Column {x:24;y:62;width:parent.width-48;spacing:14
   PanelLabel {width:parent.width;text:(root.labels[root.action] || "Session")+"?";font.pixelSize:20;font.weight:Font.DemiBold}
   PanelLabel {width:parent.width;wrapMode:Text.WordWrap;text:root.action==="suspend"?"Your session will be available when this computer wakes.":"Save your work before continuing. Open applications may close."}
   StatusMessage {width:parent.width;text:root.error;severity:"error"}
   Row {anchors.right:parent.right;spacing:10
    DialogButton {id:cancel;text:"Cancel";enabled:!worker.running;onClicked:root.visible=false}
    DialogButton {text:root.labels[root.action] || "Continue";primary:root.action==="suspend";destructive:root.action!=="suspend";enabled:!worker.running;onClicked:root.confirm()}
   }
  }
  Shortcut {sequence:"Escape";onActivated:root.visible=false}
 }
 Process {id:worker;stderr:StdioCollector {onStreamFinished:if(text.trim())root.error="The session action could not be completed. Check your system permissions."}onExited:(code,status)=>{if(code===0)root.visible=false;else root.error="The session action could not be completed. Try again after saving your work."}}
}

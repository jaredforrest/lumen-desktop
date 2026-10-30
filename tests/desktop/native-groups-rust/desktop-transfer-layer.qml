import QtQuick
import QtQuick.Window
import Quickshell
import Quickshell.Io
import Quickshell.Wayland
ShellRoot {
 property int pressed:0
 property string lastText:""
 PanelWindow {
  id:panel
  implicitWidth:160
  implicitHeight:100
  visible:true
  color:"#242426"
  WlrLayershell.namespace:"lumen-transfer-layer-test"
  WlrLayershell.layer:WlrLayer.Overlay
  WlrLayershell.keyboardFocus:WlrKeyboardFocus.Exclusive
  Item {
   id:keys
   focus:true
   Component.onCompleted:forceActiveFocus()
   Keys.onPressed:event=>{pressed++;lastText=event.text;event.accepted=true;}
  }
 }
 IpcHandler {
  target:"transferLayer"
  function status():string{return JSON.stringify({active:keys.activeFocus && keys.Window.active,visible:panel.visible,pressed:pressed,lastText:lastText});}
  function closeLayer():void{panel.visible=false;}
 }
}

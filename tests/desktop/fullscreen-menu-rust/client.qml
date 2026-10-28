import QtQuick
import Quickshell
import Quickshell.Io
ShellRoot {id:root;property int clicks:0;property int keys:0
 FloatingWindow {id:window;visible:true;title:"Fullscreen Input Probe";implicitWidth:500;implicitHeight:360;color:"#204060"
  Item {id:clientInput;anchors.fill:parent;focus:true;Keys.onPressed:event=>{root.keys++;event.accepted=true;};MouseArea {anchors.fill:parent;onClicked:root.clicks++}}
 }
 IpcHandler {target:"client";function focus():void {clientInput.forceActiveFocus();}function state():string {return JSON.stringify({clicks:root.clicks,keys:root.keys,focused:clientInput.activeFocus});}}
}

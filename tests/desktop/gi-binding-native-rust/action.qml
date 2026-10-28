import QtQuick
import Quickshell
import Quickshell.Io
import "file:@@SOURCE@@" as D
ShellRoot {property int presses:0
 FloatingWindow {visible:true;implicitWidth:320;implicitHeight:200;D.DialogButton {text:"Action sentinel";onClicked:presses++}}
 IpcHandler {target:"metadata";function count():int{return presses;}}
}

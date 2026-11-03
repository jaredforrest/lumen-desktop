import QtQuick
import Quickshell
import Quickshell.Io
ShellRoot {
    FloatingWindow {visible:true;title:"Wide corner preview";implicitWidth:2300;implicitHeight:1000;color:"#f2f2f2"}
    Timer {interval:500;running:true;onTriggered:arrange.running=true}
    Process {
        id:arrange
        command:["swaymsg","output HEADLESS-1 resolution 2560x1440; default_floating_border normal 2; client.focused #c8c8c8 #f2f2f2 #111111 #c8c8c8 #c8c8c8; corner_radius 12; [title=\"Wide corner preview\"] floating enable, border normal 2, resize set width 2300 px height 1000 px, move absolute position 100 px 100 px"]
        onExited:paint.start()
    }
    Timer {id:paint;interval:500;onTriggered:capture.running=true}
    Process {
        id:capture
        command:["grim","@CORNER_CAPTURE@"]
        onExited: code => {if(code===0)console.log("CORNER_PREVIEW_PASS");Qt.quit();}
    }
}

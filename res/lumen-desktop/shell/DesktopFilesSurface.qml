import QtQuick
import Quickshell
import Quickshell.Wayland

PanelWindow {
    id:root
    required property var shell
    required property string outputName
    property bool showFiles:false
    property bool allowKeyboard:true
    readonly property alias content:files
    anchors {left:true;right:true;top:true;bottom:true}
    visible:showFiles
    color:"transparent"
    exclusiveZone:0
    exclusionMode:ExclusionMode.Ignore
    WlrLayershell.layer:WlrLayer.Bottom
    WlrLayershell.namespace:"lumen-desktop-files"
    WlrLayershell.keyboardFocus:allowKeyboard?WlrKeyboardFocus.OnDemand:WlrKeyboardFocus.None
    mask:Region {x:files.inputArea.x;y:files.inputArea.y;width:files.inputArea.width;height:files.inputArea.height}
    onVisibleChanged:if(!visible){allowKeyboard=false;release.stop();}else allowKeyboard=true
    Timer {id:release;interval:60;onTriggered:if(root.visible)root.allowKeyboard=true}
    DesktopFiles {
        id:files
        anchors.fill:parent
        shell:root.shell
        active:root.visible
        sort:Theme.desktopPreferences.fileSort || "name"
        descending:Theme.desktopPreferences.fileDescending===true
        onKeyboardRequested:requested=>{root.allowKeyboard=requested;if(!requested)release.restart();}
    }
}

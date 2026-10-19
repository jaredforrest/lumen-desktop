import QtQuick
import QtQml.Models
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
    mask:Region {id:inputMask}
    Instantiator {
        model:files.inputRectangles
        delegate:Region {required property var modelData;x:modelData.x;y:modelData.y;width:modelData.width;height:modelData.height}
        onObjectAdded:(index,object)=>inputMask.regions.push(object)
        onObjectRemoved:(index,object)=>{const position=inputMask.regions.indexOf(object);if(position>=0)inputMask.regions.splice(position,1);}
    }
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

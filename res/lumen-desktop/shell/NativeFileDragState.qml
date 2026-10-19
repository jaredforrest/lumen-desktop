import QtQuick
import Quickshell
import Quickshell.Io

QtObject {
    id:root
    property bool active:false
    property bool available:false
    property Process observer:Process {
        running:true
        command:[Quickshell.env("GLOBAL_MENU_PYTHON") || "python3",Qt.resolvedUrl("native_file_drag.py").toString().replace(/^file:\/\//,"")]
        stdout:SplitParser {onRead:data=>{try {const value=JSON.parse(data);root.available=value.available===true;root.active=root.available && value.active===true;}catch(error){root.available=false;root.active=false;}}}
        onExited:{root.available=false;root.active=false;retry.restart();}
    }
    property Timer retry:Timer {interval:3000;onTriggered:root.observer.running=true}
}

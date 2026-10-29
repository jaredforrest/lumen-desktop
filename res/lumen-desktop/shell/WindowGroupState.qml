import QtQuick
import Quickshell
import Quickshell.Io
QtObject {
    id:root
    property var state:({available:false,enabled:false,locked:true,outputs:[]})
    function unavailable(){state={available:false,enabled:false,locked:true,outputs:[]};}
    property Process observer:Process {
        running:true
        command:["lumen-group-control","watch"]
        stdout:SplitParser {onRead:data=>{try{const value=JSON.parse(data);root.state=value;expiry.restart();}catch(error){root.unavailable();}}}
        onExited:{root.unavailable();retry.restart();}
    }
    property Timer expiry:Timer {interval:2200;onTriggered:root.unavailable()}
    property Timer retry:Timer {interval:3000;onTriggered:root.observer.running=true}
}

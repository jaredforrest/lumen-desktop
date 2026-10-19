import QtQuick
import Quickshell
import Quickshell.Io
Item {
    id:root
    property bool active:false
    property bool busy:worker.running
    property var preferences:({enabled:false,idleSeconds:300,motion:true})
    property var lock:({known:false,locked:true})
    property string error:""
    property var request:({action:"status"})
    readonly property bool permitted:lock.known===true && lock.locked===false
    function refresh(){send({action:"status"});}
    function setValue(key,value){send({action:"set",key:key,value:value});}
    function send(value){if(busy)return;request=value;worker.running=true;}
    onActiveChanged:if(active)refresh()
    Component.onCompleted:if(active)refresh()
    Timer {interval:1000;repeat:true;running:root.active;onTriggered:root.refresh()}
    Process {id:worker;stdinEnabled:true
        command:[Quickshell.env("GLOBAL_MENU_PYTHON") || "python3",Qt.resolvedUrl("screen_saver_settings.py").toString().replace(/^file:\/\//,"")]
        onStarted:write(JSON.stringify(root.request)+"\n")
        stdout:StdioCollector {onStreamFinished:{try{const data=JSON.parse(text);if(data.preferences)root.preferences=data.preferences;root.lock=data.lock || ({known:false,locked:true});root.error=data.error || "";}catch(error){root.lock=({known:false,locked:true});root.error="Unable to read screen saver preferences.";}}}
        onExited:(code,status)=>{if(code!==0){root.lock=({known:false,locked:true});root.error="Screen saver provider did not respond.";}}
    }
}

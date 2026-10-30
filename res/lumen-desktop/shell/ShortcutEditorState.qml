import QtQuick
import Quickshell.Io
Item {
    id:root
    property bool active:false
    property bool available:false
    property bool editable:false
    property bool configuredEditable:false
    property bool runtimeConfirmed:false
    property string runtimeDetail:""
    readonly property bool busy:worker.running
    property string generation:""
    property string error:""
    property string message:""
    property string boundary:""
    property var bindings:[]
    property var request:({action:"status"})
    function send(value){if(busy)return;request=value;error="";message="";worker.running=true;}
    function refresh(){send({action:"status"});}
    function change(action,id,chord,capturedGeneration){send({action:action,id:id,chord:chord,generation:capturedGeneration});}
    onActiveChanged:if(active)refresh()
    Component.onCompleted:if(active)refresh()
    Timer {interval:5000;repeat:true;running:root.active && !root.busy;onTriggered:root.refresh()}
    Process {
        id:worker;stdinEnabled:true;command:["lumen-shortcut-settings"]
        property bool received:false
        onStarted:{received=false;write(JSON.stringify(root.request)+"\n");}
        stdout:StdioCollector {onStreamFinished:{try{const data=JSON.parse(text);if(!Array.isArray(data.bindings) || typeof data.editable!=="boolean")throw Error("Invalid shortcut response");root.available=data.available===true;root.editable=data.editable;root.configuredEditable=data.configuredEditable===true;root.runtimeConfirmed=data.runtimeConfirmed===true;root.runtimeDetail=data.runtimeDetail || "";root.bindings=data.bindings;root.generation=data.generation || "";root.boundary=data.boundary || "";root.error=data.error || "";root.message=data.message || "";worker.received=true;}catch(error){root.available=false;root.editable=false;root.bindings=[];root.error="Unable to read shortcut settings.";}}}
        onExited:(code,status)=>{if(!received){root.available=false;root.editable=false;root.bindings=[];root.error="The shortcut settings provider did not respond.";}}
    }
}

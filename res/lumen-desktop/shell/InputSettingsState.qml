import QtQuick
import Quickshell
import Quickshell.Io
Item {
    id:root
    property bool active:false
    property bool available:false
    property bool busy:worker.running
    property var devices:[]
    property string error:""
    property string message:""
    property var request:({action:"status"})
    function send(value){if(busy)return;request=value;error="";message="";worker.running=true;}
    function refresh(){send({action:"status"});}
    function setValue(device,key,value){send({action:"set",device:device,key:key,value:value});}
    function restore(device,key){send({action:"restore",device:device,key:key});}
    function forget(device){send({action:"forget",device:device,confirmed:true});}
    onActiveChanged:if(active)refresh()
    Component.onCompleted:if(active)refresh()
    Timer {interval:4000;repeat:true;running:root.active;onTriggered:root.refresh()}
    Process {
        id:worker;stdinEnabled:true
        property bool received:false
        command:[Quickshell.env("GLOBAL_MENU_PYTHON") || "python3",Qt.resolvedUrl("input_settings.py").toString().replace(/^file:\/\//,"")]
        onStarted:{received=false;write(JSON.stringify(root.request)+"\n");}
        stdout:StdioCollector {onStreamFinished:{try{const data=JSON.parse(text);root.available=data.available===true;root.devices=data.devices || [];root.error=data.error || "";root.message=data.message || "";worker.received=true;}catch(error){root.available=false;root.devices=[];root.error="Unable to read input device settings.";}}}
        onExited:(code,status)=>{if(code!==0 || !received){root.available=false;root.devices=[];root.error="The input settings provider did not respond.";}}
    }
}

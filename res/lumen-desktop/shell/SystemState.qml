import QtQuick
import Quickshell
import Quickshell.Io
Item {
    id: root
    property var wifi: ({available:false,networks:[]})
    property var bluetoothRadios: ({})
    BatteryState {id:powerState}
    readonly property var power:powerState.status
    property string error: ""
    property int revision: 0
    readonly property bool busy: writer.running
    readonly property string python: Quickshell.env("GLOBAL_MENU_PYTHON") || "python3"
    readonly property string helper: Qt.resolvedUrl("system-controls.py").toString().replace(/^file:\/\//, "")
    function refresh() { if(!reader.running && !writer.running){reader.requestRevision=revision;reader.running=true;} }
    function act(action, arg) {
        if(writer.running)return;
        error="";revision++;
        writer.command=[python,helper,action].concat(arg===undefined?[]:[String(arg)]);
        writer.running=true;
    }
    Process {
        id: reader
        property int requestRevision:0
        command:[root.python,root.helper,"status"]
        onExited:(code,status)=>{if(code!==0 && reader.requestRevision===root.revision)root.wifi=({available:false,networks:[],phase:"service-unavailable",connectivity:"unknown"});}
        stdout: StdioCollector {onStreamFinished:{if(reader.requestRevision!==root.revision)return;try{const s=JSON.parse(text);powerState.publish(s.power);root.wifi=s.wifi || ({available:false,networks:[],phase:"service-unavailable",connectivity:"unknown"});if(s.bluetoothRadios)root.bluetoothRadios=s.bluetoothRadios;}catch(e){powerState.clear();root.wifi=({available:false,networks:[],phase:"service-unavailable",connectivity:"unknown"});root.error="Unable to read system controls";}}}
    }
    Process {
        id: writer
        stdout: StdioCollector {onStreamFinished:{try{const s=JSON.parse(text);if(s.error)root.error=s.error;}catch(e){root.error="Unable to complete the request";}}}
        onExited: (code,status)=>{if(code!==0 && !root.error)root.error="Unable to complete the request";Qt.callLater(root.refresh);}
    }
    Timer {interval:3000;running:true;repeat:true;triggeredOnStart:true;onTriggered:root.refresh()}
}

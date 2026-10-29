import QtQuick
import Quickshell
import Quickshell.Io
Item {
    id: root
    property var wifi: ({available:false,networks:[]})
    property var internet: ({status:"unknown",reason:"starting",message:"Checking internet connection…"})
    property var bluetoothRadios: ({})
    BatteryState {id:powerState}
    readonly property var power:powerState.status
    property string error: ""
    property int revision: 0
    readonly property bool busy: writer.running
    readonly property string helper: "lumen-system-controls"
    function refresh() { if(!reader.running && !writer.running){reader.requestRevision=revision;reader.running=true;} }
    function refreshInternet(force) {
        if(internetReader.running)return;
        internetReader.command=["desktop-network-diagnose",force?"--refresh":"--cached"];
        internetReader.running=true;
    }
    function act(action, arg) {
        if(writer.running)return;
        error="";revision++;
        writer.command=[helper,action].concat(arg===undefined?[]:[String(arg)]);
        writer.running=true;
    }
    Process {
        id: reader
        property int requestRevision:0
        command:[root.helper,"status"]
        onExited:(code,status)=>{if(code!==0 && reader.requestRevision===root.revision)root.wifi=({available:false,networks:[],phase:"service-unavailable",connectivity:"unknown"});}
        stdout: StdioCollector {onStreamFinished:{if(reader.requestRevision!==root.revision)return;try{const s=JSON.parse(text);powerState.publish(s.power);root.wifi=s.wifi || ({available:false,networks:[],phase:"service-unavailable",connectivity:"unknown"});if(s.bluetoothRadios)root.bluetoothRadios=s.bluetoothRadios;}catch(e){powerState.clear();root.wifi=({available:false,networks:[],phase:"service-unavailable",connectivity:"unknown"});root.error="Unable to read system controls";}}}
    }
    Process {
        id: writer
        stdout: StdioCollector {onStreamFinished:{try{const s=JSON.parse(text);if(s.error)root.error=s.error;}catch(e){root.error="Unable to complete the request";}}}
        onExited: (code,status)=>{if(code!==0 && !root.error)root.error="Unable to complete the request";Qt.callLater(root.refresh);}
    }
    Process {
        id: internetReader
        command:["desktop-network-diagnose","--cached"]
        stdout:StdioCollector {onStreamFinished:{
            try {const report=JSON.parse(text);root.internet=report.status?report:({status:"unknown",reason:"invalid",message:"Network monitoring returned an invalid result."});}
            catch(e){root.internet=({status:"unknown",reason:"unavailable",message:"Network monitoring is unavailable."});}
        }}
        onExited:(code,status)=>{if(code!==0)root.internet=({status:"unknown",reason:"unavailable",message:"Network monitoring is unavailable."});}
    }
    Timer {interval:3000;running:true;repeat:true;triggeredOnStart:true;onTriggered:root.refresh()}
    Timer {interval:5000;running:true;repeat:true;triggeredOnStart:true;onTriggered:root.refreshInternet(false)}
}

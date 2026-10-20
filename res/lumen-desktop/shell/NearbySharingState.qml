pragma Singleton
import QtQuick
import Quickshell
import Quickshell.Io
Singleton {
    id:root
    readonly property string label:Quickshell.env("LUMEN_NEARBY_SHARING_LABEL").trim() || "Nearby Sharing"
    readonly property bool configured:Quickshell.env("LUMEN_NEARBY_SHARING_PROVIDER")!==""
    property var state:({available:false,active:false,canSend:false,detail:""})
    property real observedAt:0
    property real now:Date.now()
    readonly property bool fresh:observedAt>0 && now-observedAt>=0 && now-observedAt<=7500
    readonly property bool available:configured && fresh && state.available
    readonly property bool active:available && state.active
    readonly property bool canSend:available && state.canSend
    readonly property string subtitle:!available?"Unavailable":state.detail || (active?"On":"Off")
    function refresh(){if(configured && !sharingReader.running)sharingReader.running=true;}
    Process {id:sharingReader;command:["lumen-nearby-sharing","status"]
        stdout:StdioCollector {onStreamFinished:{
            try {const state=JSON.parse(text);if(typeof state.available!=="boolean" || typeof state.active!=="boolean" || typeof state.canSend!=="boolean" || typeof state.detail!=="string" || state.detail.length>160 || (state.active && !state.available))throw Error("Invalid sharing status");root.state=state;root.observedAt=Date.now();root.now=root.observedAt;}
            catch(e){root.state=({available:false,active:false,canSend:false,detail:""});root.observedAt=0;}
        }}
        onExited:(code,status)=>{if(code!==0){root.state=({available:false,active:false,canSend:false,detail:""});root.observedAt=0;}}
    }
    Timer {interval:500;running:root.configured;repeat:true;onTriggered:root.now=Date.now()}
}

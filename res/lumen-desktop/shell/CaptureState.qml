import QtQuick
import Quickshell
import Quickshell.Io
Item {
    id:root
    property bool active:true
    property var status:({available:false,rows:[],error:"Capture observations are unavailable.",coverage:"Other capture paths are not assessed."})
    property double lastPublished:0
    readonly property bool audio:status.available && status.rows.some(row=>row.kind==="audio-input")
    readonly property bool video:status.available && status.rows.some(row=>row.kind==="video-input")
    function clear(){status=({available:false,rows:[],error:"Capture observations are unavailable.",coverage:"Other capture paths are not assessed."});lastPublished=0;}
    function publish(value){
        if(!value || typeof value.available!=="boolean" || !Array.isArray(value.rows) || value.rows.length>64 || typeof value.coverage!=="string" || value.coverage.length>512 || value.rows.some(row=>!row || !["audio-input","video-input"].includes(row.kind) || typeof row.reportedApplication!=="string" || row.reportedApplication.length>160 || typeof row.attribution!=="string" || row.attribution.length>256)) {clear();return false;}
        status=value.available?value:({available:false,rows:[],error:"Capture observations are unavailable.",coverage:value.coverage});lastPublished=Date.now();return true;
    }
    function expire(now){if(lastPublished && (now-lastPublished>7500 || now<lastPublished))clear();}
    function refresh(){if(active && !reader.running)reader.running=true;}
    onActiveChanged:if(!active){reader.running=false;clear();}
    Process {
        id:reader
        command:[Quickshell.env("GLOBAL_MENU_PYTHON") || "python3",Qt.resolvedUrl("capture_observation.py").toString().replace(/^file:\/\//,"")]
        stdout:StdioCollector {onStreamFinished:{try{root.publish(JSON.parse(text));}catch(error){root.clear();}}}
        onExited:(code,status)=>{if(code!==0)root.clear();}
    }
    Timer {interval:3000;running:root.active;repeat:true;triggeredOnStart:true;onTriggered:root.refresh()}
    Timer {interval:1000;running:root.active;repeat:true;onTriggered:root.expire(Date.now())}
}

import QtQuick
import Quickshell
import Quickshell.Io
Item {
    id:root
    property bool active:true
    property var status:({available:false,rows:[],error:"Capture observations are unavailable.",coverage:"Other capture paths are not assessed."})
    property double lastPublished:0
    property double lastObservedClock:0
    property int generation:0
    property var reader:null
    readonly property bool audio:status.available && status.rows.some(row=>row.kind==="audio-input")
    readonly property bool video:status.available && status.rows.some(row=>row.kind==="video-input")
    function clear(){status=({available:false,rows:[],error:"Capture observations are unavailable.",coverage:"Other capture paths are not assessed."});lastPublished=0;lastObservedClock=0;}
    function publish(value){
        if(!value || typeof value.available!=="boolean" || !Array.isArray(value.rows) || value.rows.length>64 || typeof value.coverage!=="string" || value.coverage.length>512 || value.rows.some(row=>!row || !["audio-input","video-input"].includes(row.kind) || typeof row.reportedApplication!=="string" || row.reportedApplication.length>160 || typeof row.attribution!=="string" || row.attribution.length>256)) {clear();return false;}
        status=value.available?value:({available:false,rows:[],error:"Capture observations are unavailable.",coverage:value.coverage});lastPublished=Date.now();lastObservedClock=lastPublished;return true;
    }
    function expire(now){
        if(!Number.isFinite(now)){clear();return;}
        if(status.available && (now-lastPublished>7500 || now<lastObservedClock))clear();
        lastObservedClock=now;
    }
    function refresh(){
        if(!active || reader)return;
        const process=readerFactory.createObject(root,{requestGeneration:generation});
        if(!process){clear();return;}
        reader=process;process.running=true;
    }
    onActiveChanged:if(!active){generation++;if(reader)reader.running=false;clear();}else refresh()
    Component {
        id:readerFactory
        Process {
            id:worker
            property int requestGeneration:0
            command:[Quickshell.env("GLOBAL_MENU_PYTHON") || "python3",Qt.resolvedUrl("capture_observation.py").toString().replace(/^file:\/\//,"")]
            stdout:StdioCollector {onStreamFinished:{
                if(root.reader!==worker || !root.active || worker.requestGeneration!==root.generation)return;
                try{root.publish(JSON.parse(text));}catch(error){root.clear();}
            }}
            onExited:(code,exitStatus)=>{
                if(root.reader===worker){
                    if(code!==0 && worker.requestGeneration===root.generation)root.clear();
                    root.reader=null;
                }
                const restart=root.active && worker.requestGeneration!==root.generation;
                worker.destroy();if(restart)Qt.callLater(root.refresh);
            }
        }
    }
    Timer {interval:3000;running:root.active;repeat:true;triggeredOnStart:true;onTriggered:root.refresh()}
    Timer {interval:1000;running:root.active;repeat:true;onTriggered:root.expire(Date.now())}
}

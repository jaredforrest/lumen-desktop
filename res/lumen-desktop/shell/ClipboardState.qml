import QtQuick
import Quickshell.Io
Item {
    id:root
    objectName:"clipboard-history-state"
    property string executable:"lumen-clipboard-history"
    property bool active:false
    property bool available:false
    property bool enabled:false
    property bool busy:false
    property string query:""
    property var entries:[]
    property string error:""
    property string message:""
    property int revision:0
    property bool refreshPending:false
    signal copied()
    function request(action,id) {
        if(worker.running){if(action==="list" || action==="status")refreshPending=true;return;}
        error="";message="";busy=true;
        worker.action=action;worker.requestRevision=revision;
        const args=[executable,action];
        if(action==="list")args.push("--query",query);
        if(action==="copy")args.push(id);
        worker.command=args;worker.running=true;
    }
    function search(text){query=text;revision++;entries=[];if(active && enabled)debounce.restart();}
    function enable(){request("enable");}
    function disable(){request("disable");}
    function clear(){request("clear");}
    function copy(id){if(enabled && entries.some(e=>e.id===id))request("copy",id);}
    function receive(text,action,requestRevision) {
        try {
            const data=JSON.parse(text);
            if(data.error){error=data.error;return;}
            available=true;
            if(typeof data.enabled==="boolean")enabled=data.enabled;
            if(!enabled)entries=[];
            if(action==="list" && active && requestRevision===revision) {
                if(!Array.isArray(data.entries) || data.entries.some(e=>!e || typeof e.id!=="string" || typeof e.text!=="string"))throw new Error("Invalid clipboard entries");
                entries=data.entries;
            }
            if(action==="copy" && data.copied){message="Copied. Paste it in the destination application.";copied();}
            if(action==="clear"){entries=[];message="Clipboard history cleared.";}
            if((action==="enable" || action==="status") && enabled && active)refreshPending=true;
        }catch(e){error="Clipboard history returned an unreadable response.";available=false;entries=[];}
    }
    onActiveChanged:{revision++;debounce.stop();if(active)request("status");else {entries=[];message="";refreshPending=false;}}
    Timer {id:debounce;interval:140;onTriggered:root.request("list")}
    Process {
        id:worker
        property string action:""
        property int requestRevision:0
        property string response:""
        stdout:StdioCollector {onStreamFinished:worker.response=text}
        onStarted:response=""
        onExited:(code,status)=>{
            root.busy=false;
            if(code!==0){root.error="Clipboard history is unavailable. No clipboard contents were read.";root.available=false;}
            else root.receive(response,action,requestRevision);
            if(root.refreshPending && root.active){root.refreshPending=false;Qt.callLater(()=>root.request(root.enabled?"list":"status"));}
        }
    }
}

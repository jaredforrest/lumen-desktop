import QtQuick
import Quickshell
import Quickshell.Io
Item {
    id:root
    objectName:"search-history-state"
    property bool active:false
    property bool enabled:Theme.searchPreferences.queryHistory===true
    property var queries:[]
    property string error:""
    property string generation:""
    property var pending:[]
    property bool busy:worker.running || pending.length>0
    function request(action,query) {
        if(action==="record" && (!enabled || !generation))return;
        pending=pending.concat([{action:action,query:query || "",generation:generation}]);drain();
    }
    function drain(){if(worker.running || !pending.length)return;worker.request=pending[0];pending=pending.slice(1);worker.running=true;}
    function clear(){queries=[];generation="";request("clear");}
    onActiveChanged:if(active)request("list");else queries=[]
    onEnabledChanged:{queries=[];generation="";request(enabled?"list":"clear");}
    Process {
        id:worker
        property var request:({})
        property string response:""
        command:["lumen-search-history"]
        stdinEnabled:true
        onStarted:{response="";write(JSON.stringify(request)+"\n");}
        stdout:StdioCollector {onStreamFinished:worker.response=text}
        onExited:(code,status)=>{
            try {
                const data=JSON.parse(response);
                if(code!==0 || data.error)throw new Error(data.error || "History service failed");
                if(!Array.isArray(data.queries) || data.queries.some(q=>typeof q!=="string"))throw new Error("Invalid query history");
                if(typeof data.generation!=="string" || !/^[a-f0-9]{64}$/.test(data.generation))throw new Error("Invalid history generation");
                root.generation=root.enabled && data.enabled?data.generation:"";
                root.error="";root.queries=root.active && root.enabled && data.enabled?data.queries:[];
            }catch(error){root.generation="";root.error="Search history is unavailable: "+error;root.queries=[];}
            Qt.callLater(root.drain);
        }
    }
}

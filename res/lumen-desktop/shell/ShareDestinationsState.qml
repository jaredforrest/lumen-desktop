import QtQuick
import Quickshell.Io

QtObject {
    id:root
    property var destinations:[]
    property string error:""
    property var outcome:null
    property bool readPending:false
    property bool invocationPending:false
    property var readTicket:null
    property var invocationTicket:null
    readonly property bool loading:readPending
    readonly property bool working:invocationPending
    signal finished(var result)
    function refresh(){
        if(loading || working)return;
        destinations=[];error="";outcome=null;readTicket=({});readPending=true;reader.running=true;
    }
    function invoke(request){
        if(loading || working)return false;
        error="";outcome=null;invocationTicket=({});invocationPending=true;invoker.command=["lumen-share-destinations","invoke",JSON.stringify(request)];invoker.running=true;return true;
    }
    property Process reader:Process {
        command:["lumen-share-destinations","list"]
        stdout:StdioCollector {id:advertisement}
        onRunningChanged:if(!running && root.readPending){const ticket=root.readTicket;Qt.callLater(()=>{if(root.readPending && ticket===root.readTicket && !reader.running){root.readPending=false;root.destinations=[];root.error="Destination registry could not start. Refresh or check the installed provider integration.";}});}
        onExited:(code,status)=>{
            if(!root.readPending)return;
            root.readPending=false;
            try{
                if(code!==0)throw Error("Provider registry failed");
                const value=JSON.parse(advertisement.text);
                if(value.schema!==1 || !Array.isArray(value.destinations) || value.destinations.length>4)throw Error(value.detail || "Invalid destination registry");
                const ids=[];
                for(const row of value.destinations){
                    if(typeof row.id!=="string" || ids.includes(row.id) || typeof row.label!=="string" || !row.label.length || row.label.length>80 || !["ready","offline","unavailable"].includes(row.state) || typeof row.detail!=="string" || row.detail.length>160 || typeof row.fingerprint!=="string" || (row.state==="ready" && !/^[a-f0-9]{64}$/.test(row.fingerprint)))throw Error("Invalid destination advertisement");
                    ids.push(row.id);
                }
                root.destinations=value.destinations;
            }catch(e){root.destinations=[];root.error="Destinations unavailable. Open the provider application, then refresh.";}
        }
    }
    property Process invoker:Process {
        stdout:StdioCollector {id:invocation}
        onRunningChanged:if(!running && root.invocationPending){const ticket=root.invocationTicket;Qt.callLater(()=>{if(root.invocationPending && ticket===root.invocationTicket && !invoker.running){root.invocationPending=false;root.outcome={state:"error",detail:"Provider could not start. Check its application before trying again."};root.finished(root.outcome);}});}
        onExited:(code,status)=>{
            if(!root.invocationPending)return;
            let result;
            try{
                result=JSON.parse(invocation.text);
                if(code!==0 || !["completed","cancelled","offline","error","opened","handoff"].includes(result.state) || typeof result.detail!=="string" || !result.detail.length || result.detail.length>512)throw Error("Invalid provider outcome");
                if(result.state==="handoff" && (!Array.isArray(result.command) || result.command.length<3 || result.command.length>130 || typeof result.command[0]!=="string" || !result.command[0].startsWith("/nix/store/") || result.command[1]!=="send" || !result.command.every(value=>typeof value==="string")))throw Error("Invalid provider handoff");
            }catch(e){result={state:"error",detail:"Provider outcome is unconfirmed. Check its application before trying again."};}
            root.invocationPending=false;root.outcome=result;root.finished(result);
        }
    }
}

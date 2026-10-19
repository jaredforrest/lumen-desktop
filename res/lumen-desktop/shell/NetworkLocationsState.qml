import QtQuick
import Quickshell.Io
Item {
    id:root
    objectName:"network-locations-state"
    property bool active:false
    property var command:["lumen-network-locations"]
    property bool ready:false
    property bool available:false
    property var protocols:[]
    property var connections:[]
    property string reason:""
    property string error:""
    property string selectedId:""
    property var rows:[]
    property string uri:""
    property string parentUri:""
    property string localPath:""
    property bool limited:false
    property int sequence:0
    property int intentEpoch:0
    property int operationEpoch:0
    property int operationId:0
    property int browseId:0
    property var pending:[]
    readonly property bool busy:operationId>0 || browseId>0
    readonly property var current:connections.find(connection=>connection.id===selectedId) || null
    function send(action,data){
        const id=++sequence,line=JSON.stringify(Object.assign({id:id,action:action},data || {}))+"\n";
        if(transport.running)transport.write(line);else {pending=pending.concat([line]);transport.running=true;}
        return id;
    }
    function refresh(){if(active)send("status");}
    function connectServer(address){if(busy)return;error="";rows=[];intentEpoch++;operationEpoch=intentEpoch;operationId=send("connect",{uri:address});}
    function reconnect(){if(!current || busy)return;error="";rows=[];operationEpoch=intentEpoch;operationId=send("reconnect",{connectionId:selectedId});}
    function disconnectServer(){if(!current || busy || !current.canDisconnect)return;error="";rows=[];operationId=send("disconnect",{connectionId:selectedId});}
    function cancel(){if(operationId)send("cancel",{target:operationId});if(browseId)send("cancel",{target:browseId});operationId=0;browseId=0;error="The pending request was cancelled";}
    function select(id){if(selectedId!==id){intentEpoch++;if(browseId)send("cancel",{target:browseId});browseId=0;rows=[];uri="";parentUri="";localPath="";selectedId=id;}if(current && current.state==="connected")browse();}
    function browse(address){if(!current || current.state!=="connected" || operationId)return;if(browseId)send("cancel",{target:browseId});rows=[];limited=false;localPath="";error="";browseId=send("browse",{connectionId:selectedId,uri:address || current.uri});}
    function snapshot(data){
        if(!Array.isArray(data.connections) || !Array.isArray(data.protocols))throw new Error("Invalid network state");
        connections=data.connections;protocols=data.protocols;available=data.available===true;reason=data.reason || "";
        if(selectedId && (!current || current.state!=="connected")){rows=[];localPath="";parentUri="";}
    }
    function receive(line){
        try {
            const data=typeof line==="string"?JSON.parse(line):line;
            if(data.event==="ready" || data.event==="state"){ready=true;snapshot(data);return;}
            if(data.action==="status"){snapshot(data);return;}
            if(data.id===operationId && operationId>0){
                operationId=0;error=data.error || "";
                if(data.connected && active && operationEpoch===intentEpoch){selectedId=data.connectionId;Qt.callLater(()=>{if(active && current && current.state==="connected")browse();});}
                if(data.disconnected){rows=[];localPath="";}
                return;
            }
            if(data.id===browseId && browseId>0){
                browseId=0;error=data.error || "";
                if(!active || data.error || !current || current.state!=="connected")return;
                if(!Array.isArray(data.rows) || data.rows.some(row=>!row || typeof row.name!=="string" || typeof row.uri!=="string"))throw new Error("Invalid network folder entries");
                rows=data.rows;uri=data.uri || "";parentUri=data.parent || "";localPath=data.path || "";limited=data.limited===true;
            }
        }catch(error){root.error="The network helper returned an unreadable response";rows=[];localPath="";operationId=0;browseId=0;}
    }
    onActiveChanged:{if(active)refresh();else {intentEpoch++;if(busy)cancel();rows=[];localPath="";parentUri="";}}
    Process {
        id:transport
        command:root.command
        stdinEnabled:true
        onStarted:{for(const line of root.pending)write(line);root.pending=[];}
        stdout:SplitParser {onRead:line=>root.receive(line)}
        onExited:(code,status)=>{root.ready=false;root.available=false;root.operationId=0;root.browseId=0;root.rows=[];root.localPath="";root.pending=[];if(root.active)root.error="The network helper disconnected. Refresh to retry.";}
    }
}

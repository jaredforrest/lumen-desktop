import QtQuick
import Quickshell.Io
Item {
    id:root
    objectName:"trash-actions-state"
    property var command:["lumen-trash-actions"]
    property bool active:false
    property bool busy:false
    property bool waiting:false
    property var confirmation:null
    property var operations:[]
    property var history:[]
    property string error:""
    property int sequence:0
    property int prepareId:0
    property var pending:[]
    signal changed()
    function send(action,data){const id=++sequence,line=JSON.stringify(Object.assign({id:id,action:action},data || {}))+"\n";if(transport.running)transport.write(line);else {pending=pending.concat([line]);transport.running=true;}return id;}
    function prepare(mode,items){if(waiting || busy)return;error="";retire();waiting=true;prepareId=send("trash-prepare",mode==="selected"?{mode:mode,items:items}:{mode:mode});}
    function retire(){prepareId=0;waiting=false;if(confirmation)send("trash-cancel",{token:confirmation.token});confirmation=null;}
    function confirm(){if(!confirmation || waiting)return;const token=confirmation.token;confirmation=null;waiting=true;send("trash-confirm",{token:token});}
    function refresh(){send("trash-status");}
    function stop(){send("trash-stop");}
    function inspect(id){send("trash-inspect",{operationId:id});}
    function receive(line){
        try {
            const data=JSON.parse(line);
            if(data.action==="trash-prepare"){
                if(data.id!==prepareId){if(data.token)send("trash-cancel",{token:data.token});return;}
                prepareId=0;waiting=false;if(data.error){error=data.error;return;}confirmation=data;return;
            }
            if(data.error){waiting=false;error=data.error;return;}
            if(data.action==="trash-inspect"){if(data.operation)operations=[data.operation];return;}
            if(data.action==="trash-confirm" || data.action==="trash-stop" || data.action==="trash-status"){
                const wasBusy=busy;waiting=false;busy=data.busy===true;operations=data.operations || [];history=data.history || [];
                if(!busy && (wasBusy || data.action==="trash-confirm"))changed();
            }
        }catch(problem){waiting=false;error="The Trash helper returned an unreadable result";}
    }
    onActiveChanged:{if(active)refresh();else retire();}
    Timer {interval:400;repeat:true;running:root.active || root.busy;onTriggered:root.refresh()}
    Process {id:transport;command:root.command;stdinEnabled:true
        onStarted:{for(const line of root.pending)write(line);root.pending=[];}
        stdout:SplitParser {onRead:line=>root.receive(line)}
        onExited:{root.waiting=false;root.prepareId=0;root.confirmation=null;root.pending=[];root.error="Trash helper disconnected; review retained operation results before starting another deletion";}
    }
}

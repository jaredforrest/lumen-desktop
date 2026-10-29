import QtQuick
import Quickshell
import Quickshell.Io
Item {
    id:root
    property bool active:false
    property var facts:({available:false,rows:[],coverage:[]})
    property bool busy:worker.running
    property string error:""
    property string message:""
    function refresh(){if(!busy)request({action:"list"});}
    function apply(row,action){if(!busy)request({action:action,id:row.id,revision:row.revision,confirmed:true});}
    function request(value){
        error="";message="";worker.request=JSON.stringify(value);
        worker.command=["lumen-portal-permissions"];worker.running=true;
    }
    onActiveChanged:if(active)refresh()
    Component.onCompleted:if(active)refresh()
    Timer {interval:4000;running:root.active;repeat:true;onTriggered:root.refresh()}
    Process {
        id:worker
        property string request:""
        property bool received:false
        stdinEnabled:true
        onStarted:{received=false;write(request+"\n");}
        stdout:StdioCollector {onStreamFinished:{try{const data=JSON.parse(text);root.facts=data;root.error=data.error || "";root.message=data.message || "";worker.received=true;}catch(error){root.error="The permission provider response could not be read.";root.facts={available:false,rows:[],coverage:[]};}}}
        onExited:(code,status)=>{if(code!==0 || !received){root.error="The permission provider did not respond.";root.facts={available:false,rows:[],coverage:[]};}}
    }
}

import QtQuick
import Quickshell
import Quickshell.Io
Item {
    id:root
    property bool active:false
    property bool busy:worker.running || pending.length>0
    property var preferences:({enabled:false,idleSeconds:300,motion:true})
    property var lock:({known:false,locked:true})
    property string error:""
    property var request:({action:"status"})
    readonly property bool permitted:lock.known===true && lock.locked===false
    property var pending:[]
    function refresh(){if(!busy)start({action:"status"});}
    function setValue(key,value){
        const next={action:"set",key:key,value:value};
        if(!busy){start(next);return;}
        const queue=pending.slice();
        const index=queue.findIndex(item=>item.key===key);
        if(index<0)queue.push(next);else queue[index]=next;
        pending=queue;
    }
    function start(value){request=value;worker.running=true;}
    function drain(){
        if(worker.running || pending.length===0)return;
        const queue=pending.slice();const next=queue.shift();pending=queue;start(next);
    }
    onActiveChanged:if(active)refresh()
    Component.onCompleted:if(active)refresh()
    Timer {interval:1000;repeat:true;running:root.active;onTriggered:root.refresh()}
    Process {id:worker;stdinEnabled:true
        command:["lumen-screen-saver"]
        onStarted:write(JSON.stringify(root.request)+"\n")
        stdout:StdioCollector {onStreamFinished:{try{const data=JSON.parse(text);if(data.preferences)root.preferences=data.preferences;root.lock=data.lock || ({known:false,locked:true});root.error=data.error || "";}catch(error){root.lock=({known:false,locked:true});root.error="Unable to read screen saver preferences.";}}}
        onExited:(code,status)=>{if(code!==0){root.lock=({known:false,locked:true});root.error="Screen saver provider did not respond.";}Qt.callLater(root.drain);}
    }
}

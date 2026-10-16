import QtQuick
import Quickshell
import Quickshell.Io
Item {
    id:root
    property bool active:false
    property bool available:false
    property bool busy:worker.running || pending.length>0
    property var pending:[]
    property string error:""
    property string warning:""
    property var preferences:Theme.preferences
    property var appearance:({})
    property var system:({})
    function refresh() {if(!worker.running){worker.command=["lumen-appearance","--settings"];worker.running=true;}}
    function setPreference(key,value) {
        // Coalesce only pending edits to the same key; never discard a busy edit.
        let edits=pending.filter(edit=>edit.key!==key);
        edits.push({key:key,value:value});pending=edits;
        drain();
    }
    function drain() {
        if(worker.running || pending.length===0)return;
        const edit=pending[0];pending=pending.slice(1);
        error="";warning="";worker.command=["lumen-appearance","--set",edit.key,JSON.stringify(edit.value)];worker.running=true;
    }
    onActiveChanged:if(active)refresh()
    Process {
        id:worker
        property bool received:false
        onRunningChanged:if(running)received=false
        stdout:StdioCollector {onStreamFinished:{
            try{
                const data=JSON.parse(text);worker.received=true;
                if(data.error){root.error=data.error;return;}
                root.preferences=data.preferences;root.appearance=data.appearance;root.system=data.system;
                root.available=true;root.error="";root.warning=data.warning || "";
            }catch(e){root.error="Unable to read desktop preferences";}
        }}
        onExited:(code,status)=>{if(code!==0 && !root.error)root.error="Could not save preferences";if(!received)root.error="Settings service did not respond";Qt.callLater(root.drain);}
    }
    Timer {interval:10000;running:root.active;repeat:true;onTriggered:root.refresh()}
}

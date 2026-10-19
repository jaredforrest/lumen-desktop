import QtQuick
import Quickshell
import Quickshell.Io
import "DesktopLayoutModel.js" as Layout
Item {
    id:root
    property bool active:false
    property string directory:""
    property var rows:[]
    property bool listingBusy:false
    property bool ready:false
    property var layout:({grouping:"none",positions:{}})
    property var expanded:[]
    property string error:""
    property var pending:[]
    readonly property var identities:Layout.identities(rows || [])
    readonly property bool busy:worker.running || pending.length>0
    function request(action){
        if(!active || !directory || listingBusy)return;
        const value={action:action,directory:directory,items:(identities || Layout.identities(rows || [])).slice()};
        if(action==="save")value.layout=JSON.parse(JSON.stringify(layout));
        // Consecutive pointer updates need only the newest complete snapshot.
        if(action==="save" && pending.length && pending[pending.length-1].action==="save")pending=pending.slice(0,-1);
        pending=pending.concat([value]);drain();
    }
    function drain(){if(worker.running || !pending.length)return;worker.request=pending[0];pending=pending.slice(1);worker.running=true;}
    function place(items,width,height){
        if(!ready || !items.length)return;
        const positions=Object.assign({},layout.positions),stacks=Object.assign({},layout.stacks || {});
        for(const item of items){
            const target=item.stack?stacks:positions;
            if(item.stack || layout.grouping==="none")target[item.key]=Layout.point(item.x,item.y,width,height,108,100);
        }
        layout={grouping:layout.grouping,positions:positions,stacks:stacks};request("save");
    }
    function move(key,x,y,width,height){
        if(!ready || layout.grouping!=="none")return;
        const positions=Object.assign({},layout.positions);positions[key]=Layout.point(x,y,width,height,108,100);layout={grouping:layout.grouping,positions:positions,stacks:layout.stacks || {}};request("save");
    }
    function group(mode){if(!ready || !["none","kind","modified","tags"].includes(mode))return;layout={grouping:mode,positions:layout.positions,stacks:layout.stacks || {}};expanded=[];request("save");}
    function toggle(key){expanded=expanded.includes(key)?expanded.filter(value=>value!==key):expanded.concat([key]);}
    function reload(){if(active && directory && !listingBusy)request("load");}
    onActiveChanged:if(active)reload();else {ready=false;pending=[];expanded=[];}
    onDirectoryChanged:{ready=false;pending=[];expanded=[];layout={grouping:"none",positions:{}};reload();}
    onRowsChanged:reload()
    onListingBusyChanged:if(!listingBusy)reload()
    Component.onCompleted:reload()
    Process {
        id:worker
        property var request:({})
        property string response:""
        command:[Quickshell.env("GLOBAL_MENU_PYTHON") || "python3",Qt.resolvedUrl("desktop_layout.py").toString().replace(/^file:\/\//,"")]
        stdinEnabled:true
        onStarted:{response="";write(JSON.stringify(request)+"\n");}
        stdout:StdioCollector {onStreamFinished:worker.response=text}
        onExited:(code,status)=>{
            if(root.active && request.directory===root.directory){
                try {
                    const result=JSON.parse(response);
                    if(code || result.error)throw Error(result.error || "Desktop arrangement helper failed");
                    // A queued local edit is newer than this worker response.
                    if(!root.pending.some(value=>value.action==="save"))root.layout=result.layout;
                    root.ready=true;root.error="";
                }catch(error){root.error="Desktop arrangement could not be saved: "+error;root.ready=true;}
            }
            root.drain();
        }
    }
}

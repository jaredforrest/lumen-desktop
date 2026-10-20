import QtQuick
import Quickshell
import Quickshell.Io
Item {
    id:root
    objectName:"owned-preview-state"
    property bool active:false
    property var result:({})
    property bool busy:false
    property bool retiring:false
    property bool providerReady:false
    property int sequence:0
    property int currentId:-1
    property var pending:null
    property string currentPath:""
    property int currentPage:1
    signal received(var value)
    function request(path,page,identity,token){
        const id=++sequence;currentId=path?id:-1;result={};busy=false;deadline.stop();
        currentPath=path || "";currentPage=page || 1;
        pending=path?{id:id,action:"preview",path:path,page:currentPage,identity:identity || null,token:token || null}:null;
        if(!pending || !active)return;
        busy=true;deadline.restart();startOrSend();
    }
    function startOrSend(){
        if(!active || !pending || retiring)return;
        if(!provider.running){provider.running=true;return;}
        if(providerReady){provider.write(JSON.stringify(pending)+"\n");pending=null;}
    }
    function fail(message){deadline.stop();currentId=-1;pending=null;result={error:message};busy=false;retiring=true;if(provider.running)provider.signal(9);}
    function retire(){
        ++sequence;currentId=-1;currentPath="";pending=null;result={};busy=false;deadline.stop();
        // The bindings retire Image and MediaPlayer sources before resources.
        Qt.callLater(()=>{if(!root.active && provider.running){root.retiring=true;provider.running=false;}});
    }
    onActiveChanged:if(!active)retire();else if(pending){busy=true;deadline.restart();startOrSend();}
    Timer {id:deadline;interval:10000;onTriggered:{root.fail("Preview timed out. Select the item again.");}}
    Process {
        id:provider;stdinEnabled:true
        command:[Quickshell.env("GLOBAL_MENU_PYTHON") || "python3",Qt.resolvedUrl("owned_preview.py").toString().replace(/^file:\/\//,"")]
        onStarted:{root.providerReady=true;if(!root.active){root.retiring=true;provider.running=false;}else root.startOrSend();}
        stdout:SplitParser {onRead:line=>{
            try {
                const data=JSON.parse(line);
                if(root.retiring || !root.active || data.id!==root.currentId || data.sourcePath!==root.currentPath || (!data.error && data.requestedPage!==root.currentPage))return;
                deadline.stop();root.busy=false;root.result=data;root.received(data);
            }catch(e){root.fail("Preview returned an unreadable response");}
        }}
        onExited:{
            deadline.stop();root.providerReady=false;const restart=root.retiring && root.active && root.pending;
            const failure=root.result.error;root.retiring=false;root.busy=false;root.result={};
            if(restart){root.busy=true;deadline.restart();Qt.callLater(root.startOrSend);}
            else if(root.active && (root.currentId>0 || failure)){root.currentId=-1;root.result={error:failure || "Preview disconnected. Select the item again."};}
        }
    }
}

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
    property string providerGeneration:""
    property int sequence:0
    property int currentId:-1
    property var pending:null
    property string currentPath:""
    property int currentPage:1
    property var outstanding:[]
    property var releases:[]
    property int releaseId:-1
    property bool stopRequested:false
    property int stopSignal:0
    property alias resourceRetirement:retirement
    signal received(var value)
    function retireCurrent(){
        if(outstanding.length)retirement.retire(outstanding,result.uri || "");
        outstanding=[];result={};
    }
    function request(path,page,identity,token){
        retireCurrent();
        const id=++sequence;currentId=path?id:-1;busy=false;deadline.stop();
        currentPath=path || "";currentPage=page || 1;
        pending=path?{id:id,action:"preview",path:path,page:currentPage,identity:identity || null,token:token || null}:null;
        if(!pending || !active){if(!retirement.busy)dispatchReleases();return;}
        stopRequested=false;busy=true;deadline.restart();startOrSend();
    }
    function dispatchReleases(){
        if(retirement.busy || releaseId>0)return;
        if(stopRequested){tryStop();return;}
        if(!provider.running){releases=[];startOrSend();return;}
        if(!providerReady)return;
        if(releases.length){
            releaseId=++sequence;
            provider.write(JSON.stringify({id:releaseId,action:"retire-preview",generation:providerGeneration,requests:releases,keepSourceToken:pending?pending.token:null})+"\n");releases=[];
        }else startOrSend();
    }
    function startOrSend(){
        if(!active || !pending || retiring || retirement.busy || releaseId>0)return;
        if(releases.length){dispatchReleases();return;}
        if(!provider.running){provider.running=true;return;}
        if(providerReady){
            outstanding=outstanding.concat([pending.id]);
            provider.write(JSON.stringify(Object.assign({generation:providerGeneration},pending))+"\n");pending=null;deadline.restart();
        }
    }
    function tryStop(){
        if(!stopRequested || retirement.busy)return;
        releases=[];releaseId=-1;
        if(provider.running){retiring=true;if(stopSignal)provider.signal(stopSignal);else provider.running=false;}
    }
    function fail(message){
        deadline.stop();currentId=-1;pending=null;retireCurrent();result={error:message};busy=false;stopRequested=true;stopSignal=9;tryStop();
    }
    function retire(){
        ++sequence;currentId=-1;currentPath="";pending=null;retireCurrent();busy=false;deadline.stop();stopRequested=true;stopSignal=0;
        Qt.callLater(root.tryStop);
    }
    onActiveChanged:if(!active)retire();else if(pending){stopRequested=false;busy=true;deadline.restart();startOrSend();}
    PreviewResourceRetirement {id:retirement;onRetired:ids=>{root.releases=[...new Set(root.releases.concat(ids))];root.dispatchReleases();}}
    Timer {id:deadline;interval:10000;onTriggered:{root.fail("Preview timed out. Select the item again.");}}
    Process {
        id:provider;stdinEnabled:true
        command:[Quickshell.env("GLOBAL_MENU_PYTHON") || "python3",Qt.resolvedUrl("owned_preview.py").toString().replace(/^file:\/\//,"")]
        onStarted:{if(!root.active){root.stopRequested=true;root.tryStop();}}
        stdout:SplitParser {onRead:line=>{
            try {
                const data=JSON.parse(line);
                if(data.event==="ready"){
                    if(typeof data.generation!=="string" || !/^[a-f0-9]{48}$/.test(data.generation))throw new Error("Invalid preview provider identity");
                    root.providerGeneration=data.generation;root.providerReady=true;
                    if(!root.active)root.tryStop();else root.dispatchReleases();
                    return;
                }
                if(data.generation!==root.providerGeneration)return;
                if(data.action==="retire-preview" && data.id===root.releaseId){
                    if(data.error || data.retired!==true){root.fail("Preview resources could not be retired");return;}
                    root.releaseId=-1;root.dispatchReleases();return;
                }
                if(root.retiring || !root.active || data.id!==root.currentId || data.sourcePath!==root.currentPath || (!data.error && data.requestedPage!==root.currentPage))return;
                deadline.stop();root.busy=false;root.result=data;root.received(data);
            }catch(e){root.fail("Preview returned an unreadable response");}
        }}
        onExited:{
            deadline.stop();root.providerReady=false;root.providerGeneration="";root.releaseId=-1;root.releases=[];
            const restart=root.active && root.pending;const failure=root.result.error;
            root.retireCurrent();root.retiring=false;root.busy=false;
            if(restart){root.stopRequested=false;root.busy=true;deadline.restart();Qt.callLater(root.startOrSend);}
            else if(root.active && (root.currentId>0 || failure)){root.currentId=-1;root.result={error:failure || "Preview disconnected. Select the item again."};}
        }
    }
}

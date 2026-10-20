import QtQuick
import Quickshell
import Quickshell.Io
Item {
    id: root
    property bool active: false
    property bool available: false
    property bool scanning: false
    property bool limited: false
    property int indexed: 0
    property int unreadable: 0
    property string query: ""
    property var files: []
    property bool more: false
    property bool searching: false
    property bool opening: false
    property string error: ""
    property var preview: ({})
    property string previewPath: ""
    property string previewToken: ""
    property int sequence: 0
    property int searchId: -1
    property int previewId: -1
    property int openId: -1
    property bool timedOut: false
    property bool retiring: false
    property string previewGeneration:""
    property var previewRequests:[]
    property var previewReleases:[]
    property int previewReleaseId:-1
    property bool stopAfterRetirement:false
    property bool killAfterRetirement:false
    property alias resourceRetirement:previewRetirement
    signal opened()
    signal browseRequested(string path)
    readonly property string python: Quickshell.env("GLOBAL_MENU_PYTHON") || "python3"
    property string helper: Qt.resolvedUrl("search.py").toString().replace(/^file:\/\//, "")
    function send(action, data) {
        if (!available) return -1;
        const id=++sequence;
        responseDeadline.restart();
        transport.write(JSON.stringify(Object.assign({id:id,action:action},data || {}))+"\n");
        return id;
    }
    function search(text) {
        query=text;files=[];more=false;error="";searchId=-1;openId=-1;opening=false;searching=active && text.trim().length>0;
        indexRefresh.stop();
        requestPreview("");
        if(searching)debounce.restart();else debounce.stop();
    }
    function retirePreview(){
        if(previewRequests.length)previewRetirement.retire(previewRequests,preview.uri || "");
        previewRequests=[];preview={};previewId=-1;
    }
    function requestPreview(path, token) {
        if(path && path===previewPath && (token || "")===previewToken && (previewId>0 || previewDelay.running || preview.action==="preview"))return;
        retirePreview();previewToken=token || "";previewPath=path;
        if(active && path)previewDelay.restart();else previewDelay.stop();
    }
    function dispatchPreviewReleases(){
        if(previewRetirement.busy || previewReleaseId>0)return;
        if(stopAfterRetirement){tryStopTransport();return;}
        if(!transport.running){previewReleases=[];return;}
        if(!available || !previewGeneration)return;
        if(previewReleases.length){
            previewReleaseId=send("retire-preview",{previewGeneration:previewGeneration,requests:previewReleases,keepSourceToken:active?previewToken:null});previewReleases=[];
        }else if(active && previewPath)previewDelay.restart();
    }
    function tryStopTransport(){
        if(!stopAfterRetirement || previewRetirement.busy)return;
        previewReleases=[];previewReleaseId=-1;
        if(transport.running){retiring=true;if(killAfterRetirement)transport.signal(9);else transport.running=false;}
    }
    function open(path, parent, token, browse) {
        if(opening || !active || !available || !token)return;
        error="";opening=true;openId=send(browse?"browse":parent?"parent":"open",{path:path,token:token,parent:!!parent});
    }
    function refresh() { if(available)send("refresh"); }
    onActiveChanged: {
        if(active && !retiring && !transport.running)transport.running=true;
        if(active && !available)responseDeadline.restart();
        if(active && available && query.trim()) {searching=true;debounce.restart();}
        if(!active){responseDeadline.stop();debounce.stop();indexRefresh.stop();previewDelay.stop();reconnect.stop();searchId=-1;openId=-1;opening=false;retirePreview();previewPath="";previewToken="";files=[];more=false;searching=false;available=false;stopAfterRetirement=true;killAfterRetirement=false;retiring=transport.running;Qt.callLater(root.tryStopTransport);}
    }
    PreviewResourceRetirement {id:previewRetirement;onRetired:ids=>{root.previewReleases=[...new Set(root.previewReleases.concat(ids))];root.dispatchPreviewReleases();}}
    Timer {id:debounce;interval:140;onTriggered: {
        if(root.available && root.active && root.query.trim())root.searchId=root.send("search",{query:root.query});
    }}
    Timer {id:previewDelay;interval:160;onTriggered: {
        if(root.available && root.active && root.previewPath && root.previewGeneration && !previewRetirement.busy && root.previewReleaseId<0 && !root.previewReleases.length){root.previewId=root.send("preview",{path:root.previewPath,token:root.previewToken,previewGeneration:root.previewGeneration});root.previewRequests=root.previewRequests.concat([root.previewId]);}
    }}
    Timer {id:indexRefresh;interval:400;onTriggered: {
        if(root.available && root.active && root.query.trim())root.searchId=root.send("search",{query:root.query});
    }}
    function receive(line) {
        if(retiring)return;
            try {
                const data=typeof line === "string" ? JSON.parse(line) : line;
                if(data.event==="ready" || data.event==="index") {
                    if(data.event==="ready"){root.available=true;root.previewGeneration=typeof data.previewGeneration==="string" && /^[a-f0-9]{48}$/.test(data.previewGeneration)?data.previewGeneration:"";root.timedOut=false;root.error="";debounce.restart();}
                    root.scanning=data.scanning;root.indexed=data.count;root.limited=data.limited;root.unreadable=data.unreadable;
                    if(root.active && root.query.trim())indexRefresh.restart();
                } else if(data.action==="retire-preview" && data.id===root.previewReleaseId && data.previewGeneration===root.previewGeneration){
                    root.previewReleaseId=-1;
                    if(data.error || data.retired!==true){root.error="Preview resources could not be retired";root.stopAfterRetirement=true;root.killAfterRetirement=true;root.tryStopTransport();}
                    else root.dispatchPreviewReleases();
                } else if(root.active && root.searchId>0 && data.id===root.searchId) {
                    root.searchId=-1;root.searching=false;root.error=data.error || "";root.files=data.results || [];root.more=data.more || false;
                } else if(root.active && root.previewId>0 && data.id===root.previewId) {
                    root.previewId=-1;root.preview=data.error?{type:"generic",text:data.error,reason:data.reason || "provider_failure"}:data;
                } else if(root.active && root.openId>0 && data.id===root.openId) {
                    root.openId=-1;root.opening=false;root.error=data.error || "";if(data.opened)root.opened();else if(data.browse)root.browseRequested(data.browse);
                }
                if(root.available && root.searchId<0 && root.previewId<0 && root.openId<0 && root.previewReleaseId<0)responseDeadline.stop();
            } catch(e){root.error="Unable to read file search results";}
    }
    Process {
        id:transport
        command:[root.python,root.helper]
        stdinEnabled:true
        stdout:SplitParser {onRead:line=>{
            root.receive(line);
        }}
        onExited:(code,status)=>{
            responseDeadline.stop();root.retirePreview();root.retiring=false;root.available=false;root.previewGeneration="";root.previewReleaseId=-1;root.previewReleases=[];root.stopAfterRetirement=false;root.killAfterRetirement=false;root.searchId=-1;root.openId=-1;root.searching=false;root.opening=false;root.previewPath="";root.previewToken="";root.files=[];root.more=false;
            if(root.active){root.error=root.timedOut?"File search timed out. Reconnecting…":"File search disconnected. Reconnecting…";reconnect.restart();}
        }
    }
    // A bounded request/startup deadline retires only this owned subprocess.
    // Read-only queries may reconnect; an open action is never retried.
    Timer {id:responseDeadline;interval:10000;onTriggered:{
        root.timedOut=true;root.available=false;root.searchId=-1;root.openId=-1;
        root.searching=false;root.opening=false;root.files=[];root.more=false;root.retirePreview();
        root.previewPath="";root.previewToken="";root.error="File search is unavailable";
        root.stopAfterRetirement=true;root.killAfterRetirement=true;root.tryStopTransport();
    }}
    Timer {id:reconnect;interval:2000;onTriggered:if(root.active){if(previewRetirement.busy)restart();else {responseDeadline.restart();transport.running=true;}}}
}

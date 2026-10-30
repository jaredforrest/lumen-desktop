import QtQuick
import Quickshell
import Quickshell.Io
import "FilesHistoryModel.js" as HistoryModel
Item {
    id:root
    signal informationReady(var details)
    signal batchRenameReady(var reply)
    property bool active:false
    property bool ready:false
    property var queued:[]
    property string path:Quickshell.env("HOME")
    property string parentPath:""
    property var rows:[]
    property string rowsPath:""
    property int committedListingId:0
    property bool committedListingComplete:false
    readonly property bool listingComplete:listingCurrent && committedListingComplete
    readonly property bool listingCurrent:ready && !busy && committedListingId>0 && committedListingId===currentId && rowsPath===path
    function prepareTab(value){path=value;rows=[];rowsPath="";breadcrumbs=[];parentPath="";columns=[];columnRequests={};}
    property var breadcrumbs:[]
    property bool busy:false
    property bool pendingRefresh:false
    property string error:""
    property string warning:""
    property bool writable:false
    property bool hidden:false
    property var favorites:null
    property int sidebarWidth:190
    property int previewWidth:240
    property var columnWidths:[]
    onPreviewWidthChanged:savePreferences()
    onColumnWidthsChanged:savePreferences()
    property var collapsedSidebarGroups:[]
    onSidebarWidthChanged:savePreferences()
    onCollapsedSidebarGroupsChanged:savePreferences()
    property bool foldersFirst:true
    property string groupBy:"none"
    property string sort:"name"
    property string viewMode:"grid"
    property bool preferencesLoaded:false
    property bool persistViewPreferences:true
    property var columns:[]
    property var columnRequests:({})
    property bool restoringPresentation:false
    function restoreView(value){restoringPresentation=true;viewMode=value;restoringPresentation=false;}
    function savePreferences(){if(preferencesLoaded && persistViewPreferences && !restoringPresentation)send("save-preferences",{preferences:{view:viewMode,sort:sort,descending:descending,hidden:hidden,foldersFirst:foldersFirst,groupBy:groupBy,favorites:favorites,sidebarWidth:sidebarWidth,previewWidth:previewWidth,columnWidths:columnWidths,collapsedSidebarGroups:collapsedSidebarGroups}});}
    function loadColumns(){
        columnRequests={};columns=[];if(viewMode!=="columns" || path==="trash:")return;
        const ancestors=breadcrumbs.slice(-3);columns=ancestors.map(c=>({path:c.path,name:c.name,rows:[],loaded:false}));
        let requests={};ancestors.forEach((c,i)=>{requests[send("column-list",{path:c.path,query:c.path===path?query:"",hidden:hidden,sort:sort,descending:descending,foldersFirst:foldersFirst,groupBy:groupBy})]=i;});columnRequests=requests;
    }
    onViewModeChanged:{savePreferences();loadColumns();}
    onFavoritesChanged:savePreferences()
    onFoldersFirstChanged:savePreferences()
    onGroupByChanged:savePreferences()
    onHiddenChanged:savePreferences()
    onSortChanged:savePreferences()
    onDescendingChanged:savePreferences()
    property bool descending:false
    property string query:""
    property var history:[]
    property int historyIndex:-1
    property var preview:ownedPreview.result
    readonly property bool previewBusy:ownedPreview.busy
    readonly property var previewResourceRetirement:ownedPreview.resourceRetirement
    OwnedPreviewState {id:ownedPreview;active:root.active && root.ready}
    property int sequence:0
    readonly property string requestSession: Date.now().toString(36)+"-"+Math.random().toString(36).slice(2)
    property int currentId:0
    property string nextSelection:""
    property string stagedPath:""
    property string stagedAction:"copy"
    property var stagedPaths:[]
    property var jobs:[]
    property var jobStates:({})
    property real createdAt:Date.now()/1000
    property var volumes:[]
    property bool canUndo:false
    property string undoKind:""
    property string undoLabel:"Undo Last Move"
    readonly property var currentJob:jobs.length?jobs[jobs.length-1]:null
    readonly property bool working:jobs.some(j=>j.state==="running" || j.state==="cancelling")
    property string requestedPath:""
    property int requestedHistory:-1
    signal capturePresentation()
    signal historyReady(var presentation)
    signal selectedPath(string path)
    signal applicationsReady(var applications,string path,var selection)
    property int applicationRequestId:0
    property string applicationRequestPath:""
    property var applicationRequestFacts:[]
    function send(action,data) {
        const id=++sequence;
        if(["job-start","clipboard-paste","drop"].includes(action))data=Object.assign({requestId:requestSession+"-"+id},data || {});
        const line=JSON.stringify(Object.assign({id:id,action:action},data || {}))+"\n";
        if(ready)transport.write(line);else {queued=queued.concat([line]);if(!transport.running)transport.running=true;}
        return id;
    }
    function navigate(value,index,preserveError,expectedFacts) {
        if(!busy)capturePresentation();
        busy=true;if(!preserveError)error="";requestedPath=value || path;
        requestedHistory=index===undefined?-1:index;
        const target=requestedHistory>=0?history[requestedHistory]:null;
        if(target && target.presentation)query=target.presentation.query || "";
        currentId=send("list",Object.assign({path:requestedPath,query:query,hidden:hidden,sort:sort,descending:descending,foldersFirst:foldersFirst,groupBy:groupBy},expectedFacts?{expectedFacts:expectedFacts.slice()}:{}));
    }
    function refresh(){if(busy){pendingRefresh=true;return;}navigate(path,historyIndex,true);}
    function back(delta){const i=historyIndex+delta;if(i>=0 && i<history.length){navigate(HistoryModel.path(history[i]),i);}}
    function cancelApplications(){applicationRequestId=0;applicationRequestPath="";applicationRequestFacts=[];}
    function act(action,data) {
        error="";
        if(["copy","move","trash","restore"].includes(action)){startJob(action,data.paths || [data.path],data.destination);return;}
        if(action==="applications"){const row=rows.find(item=>item.path===data.path);if(!row || !row.actionFacts){error="Refresh the selected file before choosing an application";return;}applicationRequestPath=data.path;applicationRequestFacts=row.actionFacts.slice();applicationRequestId=send(action,Object.assign({},data,{facts:row.actionFacts}));return;}
        if(["retire-actions","open-with","set-default","eject","open"].includes(action)){send(action,data);return;}
        busy=true;currentId=send(action,data);
    }
    function startJob(operation,paths,destination,conflict){error="";send("job-start",{operation:operation,paths:paths || [],destination:destination || path,conflict:conflict || "skip"});}
    function requestPreview(value,page) {const row=rows.find(item=>item.path===value);const token=page!==undefined && preview.sourcePath===value?preview.sourceToken:null;ownedPreview.request(value,page,row?row.identity:null,token);}
    function stage(paths,action){stagedPaths=paths;stagedPath=paths[0] || "";stagedAction=action;error="";send("clipboard-write",{paths:paths,operation:action});}
    function paste(conflict){error="";send("clipboard-paste",{destination:path,conflict:conflict || "skip"});}
    function drop(urls,destination){error="";send("drop",{uris:urls.join("\n"),destination:destination || path});}
    function cancelJob(){if(currentJob)send("job-cancel",{job:currentJob.id});}
    function undo(){startJob("undo",[]);}
    function acceptJobs(data){
        let states=Object.assign({},jobStates),changed=false;
        for(const job of data.jobs || []){
            if(states[job.id]!==job.state && !["running","cancelling"].includes(job.state)){
                if(states[job.id] || job.started>=createdAt){changed=true;if(job.state!=="complete" && job.recoveryPath && job.undoMessage)error=job.undoMessage+"\nRecovery: "+job.recoveryPath;else if(job.errors.length)error=job.errors.join("\n");else if(job.state==="complete" && job.operation==="undo")error="";if(job.undoWarning)warning=job.undoWarning;}
            }
            states[job.id]=job.state;
        }
        jobs=data.jobs || [];jobStates=states;if(data.historyWarning)error=data.historyWarning;for(const job of jobs){if(job.state==="interrupted" && jobStates[job.id]!=="interrupted")error=job.recoveryPath && job.undoMessage?job.undoMessage+"\nRecovery: "+job.recoveryPath:job.errors.join("\n");}canUndo=!!data.canUndo;undoKind=["copy","move"].includes(data.undoKind)?data.undoKind:"";undoLabel=typeof data.undoLabel==="string"?data.undoLabel:"Undo Last Move";
        if(changed)refresh();
    }
    onActiveChanged:if(active){if(persistViewPreferences && !preferencesLoaded)send("preferences");send("job-status");send("volumes");}
    Timer {id:watchRefresh;interval:200;onTriggered:root.refresh()}
    Timer {interval:600;running:root.ready && (root.active || root.working);repeat:true;onTriggered:root.send("job-status")}
    Timer {interval:10000;running:root.ready && root.active;repeat:true;onTriggered:root.send("volumes")}
    Process {
        id:transport;stdinEnabled:true
        onStarted:{root.ready=true;for(const line of root.queued)write(line);root.queued=[];}
        command:[Quickshell.env("LUMEN_FILES_BIN") || "lumen-files-rust-wip"]
        stdout:SplitParser {onRead:line=>{
            try {
                const data=JSON.parse(line);
                if(data.action==="info"){root.informationReady(data.info || {error:data.error});return;}
                if(["rename-preview","rename-confirm","rename-cancel"].includes(data.action)){root.batchRenameReady(data);if(data.action==="rename-confirm")root.refresh();return;}
                if(data.action==="preferences"){root.restoringPresentation=true;const p=data.preferences || {};root.viewMode=p.view || "grid";root.sort=p.sort || "name";root.descending=!!p.descending;root.hidden=!!p.hidden;root.foldersFirst=p.foldersFirst!==false;root.groupBy=p.groupBy || "none";root.favorites=p.favorites===undefined?null:p.favorites;root.sidebarWidth=Number.isInteger(p.sidebarWidth)?Math.max(120,Math.min(360,p.sidebarWidth)):190;root.previewWidth=Number.isInteger(p.previewWidth)?Math.max(160,Math.min(600,p.previewWidth)):240;root.columnWidths=Array.isArray(p.columnWidths) && p.columnWidths.length<=16?p.columnWidths.map(value=>Number.isInteger(value)?Math.max(160,Math.min(600,value)):240):[];root.collapsedSidebarGroups=Array.isArray(p.collapsedSidebarGroups)?p.collapsedSidebarGroups.filter(group=>["favourites","locations"].includes(group)):[];root.preferencesLoaded=true;root.restoringPresentation=false;root.refresh();return;}
                if(data.action==="save-preferences")return;
                if(data.action==="column-list"){if(data.id in root.columnRequests){const columns=root.columns.slice();const i=root.columnRequests[data.id];columns[i]=Object.assign({},columns[i],{rows:data.rows || [],error:data.error || "",loaded:true});root.columns=columns;}return;}
                if(data.event==="changed"){watchRefresh.restart();return;}
                if(data.jobs){root.acceptJobs(data);return;}
                if(data.action==="volumes"){root.volumes=data.volumes || [];return;}
                if(data.action==="applications"){if(data.id!==root.applicationRequestId){if(data.capability)root.send("retire-actions",{capabilities:[data.capability]});return;}root.applicationRequestId=0;if(data.error)root.error=data.error;else root.applicationsReady(data.applications,root.applicationRequestPath,{capability:data.capability,session:data.generation,choice:data.choice,facts:root.applicationRequestFacts.slice()});return;}
                if(data.id!==root.currentId){if(data.action!=="list" && data.error)root.error=data.error;return;}
                root.busy=false;
                if(data.error){root.committedListingId=0;root.error=data.error;if(data.action==="list"){const saved=(root.history[root.historyIndex] || {}).presentation;if(saved && root.rowsPath===root.path){root.query=saved.query;root.historyReady(saved);root.selectedPath("@history");}}return;}
                if(data.action==="list"){
                    const sameFolder=root.path===data.path;
                    root.path=data.path;root.rowsPath=data.path;root.parentPath=data.parent;root.rows=data.rows;root.breadcrumbs=data.breadcrumbs;root.writable=data.writable;root.loadColumns();
                    root.warning=data.limited?"Showing up to 5,000 matches. Narrow the folder filter.":data.unreadable?"Some items could not be read.":"";
                    const committed=HistoryModel.commit(root.history,root.historyIndex,data.path,root.requestedHistory,root.requestSession+"-"+data.id);
                    root.history=committed.history;root.historyIndex=committed.index;
                    const saved=(root.history[root.historyIndex] || {}).presentation;
                    root.historyReady(saved || null);
                    root.selectedPath(root.nextSelection || (saved?"@history":sameFolder?"@preserve":""));root.nextSelection="";
                    if(root.pendingRefresh){root.pendingRefresh=false;Qt.callLater(root.refresh);}
                    root.committedListingComplete=!data.limited && !data.unreadable;
                    root.committedListingId=data.id===root.currentId && root.rowsPath===root.path?data.id:0;
                } else {
                    root.nextSelection=data.selected || "";root.refresh();
                }
            }catch(e){root.committedListingId=0;root.busy=false;root.error="Unable to read Files results: "+e;}
        }}
        onExited:(code,status)=>{root.cancelApplications();root.committedListingId=0;root.ready=false;root.queued=[];root.busy=false;root.error="Files disconnected. Refresh to reconnect.";}
    }
}

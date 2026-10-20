import QtQuick
import Quickshell.Io
Item {
    id:root
    required property var ownerWindow
    property string persistenceKey:""
    property int watchEpoch:0
    property bool restorationAttempted:false
    property bool ownedFitPending:false
    property bool persistenceSupported:false
    property string persistenceError:""
    readonly property string persistenceStatus:error || (persistenceKey && Theme.windowsPreferences.rememberOwnedGeometry?(persistenceError || (watchReady && !persistenceSupported?"Remembered placement requires the updated compositor at next login":"")):"")
    property int containerId:0
    property int preferredWidth:Math.floor(ownerWindow.implicitWidth)
    property int preferredHeight:Math.floor(ownerWindow.implicitHeight)
    property string appId:"org.quickshell"
    property var pending:null
    property int widthMargin:-1
    property int heightMargin:-1
    property bool watchReady:false
    readonly property bool marginMode:widthMargin>=0 && heightMargin>=0
    property var result:({})
    property string error:""
    property int completedRequests:0
    readonly property bool busy:worker.running || ownedFitPending || pending!==null
    function fitWorkspace(width,height){widthMargin=width;heightMargin=height;fit(1,1);}
    function restartWatch(){watchEpoch++;watcher.running=false;watchReady=false;Qt.callLater(()=>{if(ownerWindow.visible && marginMode)watcher.running=true;});}
    function fit(outerWidth,outerHeight) {
        if(!ownerWindow.visible)return;
        pending={width:Math.floor(outerWidth),height:Math.floor(outerHeight),title:ownerWindow.title,container:containerId,preferredWidth:preferredWidth,preferredHeight:preferredHeight};
        drain.restart();
    }
    Timer {id:drain;interval:0;onTriggered:{
        if(worker.running || root.ownedFitPending || !root.pending || ((root.marginMode || root.persistenceKey) && !root.watchReady))return;
        const request=root.pending;root.pending=null;
        if(!root.ownerWindow.visible || request.title!==root.ownerWindow.title)return;
        root.error="";
        if(root.persistenceKey){
            if(!root.watchReady)return;
            root.ownedFitPending=true;
            watcher.write(JSON.stringify({action:"fit",width:request.width,height:request.height,minimumWidth:Math.floor(root.ownerWindow.minimumSize.width),minimumHeight:Math.floor(root.ownerWindow.minimumSize.height),preferredWidth:request.preferredWidth,preferredHeight:request.preferredHeight})+"\n");
            return;
        }
        worker.received=false;
        worker.command=["lumen-window-geometry","--app",root.appId,"--title",request.title,"--container",String(request.container),"--outer-width",String(request.width),"--outer-height",String(request.height),"--minimum-width",String(root.ownerWindow.minimumSize.width),"--minimum-height",String(root.ownerWindow.minimumSize.height)];
        if(request.preferredWidth>0 && request.preferredHeight>0)worker.command=worker.command.concat(["--preferred-width",String(request.preferredWidth),"--preferred-height",String(request.preferredHeight)]);
        if(root.marginMode)worker.command=worker.command.concat(["--margin-width",String(root.widthMargin),"--margin-height",String(root.heightMargin)]);
        worker.running=true;
    }}
    Process {id:worker
        property bool received:false
        stdout:StdioCollector {onStreamFinished:{try{const response=JSON.parse(text);worker.received=true;if(response.error)root.error=response.error;else root.result=response;}catch(e){root.error="Invalid window geometry response";}}}
        onExited:(code,status)=>{root.completedRequests++;if(root.ownerWindow.visible && !received)root.error="Window geometry service did not respond";if(root.pending)drain.restart();}
    }
    onMarginModeChanged:if(marginMode && ownerWindow.visible)restartWatch()
    onContainerIdChanged:if(marginMode && ownerWindow.visible)restartWatch()
    function watchCommand(){
        let command=["lumen-window-geometry","--watch","--app",root.appId,"--title",root.ownerWindow.title,"--container",String(root.containerId)];
        if(persistenceKey){
            command=command.concat(["--owner-key",persistenceKey,"--observer-token",String(watchEpoch),"--minimum-width",String(Math.floor(ownerWindow.minimumSize.width)),"--minimum-height",String(Math.floor(ownerWindow.minimumSize.height))]);
            if(!restorationAttempted)command.push("--restore-owned");
            if(marginMode)command=command.concat(["--margin-width",String(widthMargin),"--margin-height",String(heightMargin)]);
        }
        return command;
    }
    Process {id:watcher
        stdinEnabled:!!root.persistenceKey
        command:root.watchCommand()
        stdout:SplitParser {onRead:line=>{try{const message=JSON.parse(line);if(root.persistenceKey && message.observer!==root.watchEpoch)return;if(message.ready){root.watchReady=true;root.persistenceSupported=message.persistenceSupported===true;root.restorationAttempted=true;root.persistenceError=message.restorationError || "";if(root.pending)drain.restart();}else if(message.fitted){root.ownedFitPending=false;root.completedRequests++;if(message.error)root.error=message.error;else root.result=message.result;if(root.pending)drain.restart();}else if(message.storageError){root.persistenceError=message.storageError;}else if(message.remembered){root.persistenceError="";}else if(message.changed && message.app===root.appId && message.title===root.ownerWindow.title && (!root.containerId || message.container===root.containerId)){root.ownerWindow.fitScreen();}else if(message.error)root.error=message.error;}catch(e){root.error="Invalid geometry observer response";}}}
        onExited:(code,status)=>{root.watchReady=false;root.ownedFitPending=false;}
    }
    Connections {target:root.ownerWindow
        function onVisibleChanged(){if(!root.ownerWindow.visible){root.restorationAttempted=false;root.watchEpoch++;root.pending=null;root.ownedFitPending=false;worker.running=false;watcher.running=false;root.watchReady=false;}else if(root.marginMode)root.restartWatch();}
        function onTitleChanged(){if(root.marginMode && root.ownerWindow.visible){root.restartWatch();root.ownerWindow.fitScreen();}}
    }
}

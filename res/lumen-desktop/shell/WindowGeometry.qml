import QtQuick
import Quickshell.Io
Item {
    id:root
    required property var ownerWindow
    property int containerId:0
    property string appId:"org.quickshell"
    property var pending:null
    property var result:({})
    property string error:""
    property int completedRequests:0
    readonly property bool busy:worker.running || pending!==null
    function fit(outerWidth,outerHeight) {
        if(!ownerWindow.visible)return;
        pending={width:Math.floor(outerWidth),height:Math.floor(outerHeight),title:ownerWindow.title,container:containerId};
        drain.restart();
    }
    Timer {id:drain;interval:0;onTriggered:{
        if(worker.running || !root.pending)return;
        const request=root.pending;root.pending=null;
        if(!root.ownerWindow.visible || request.title!==root.ownerWindow.title)return;
        root.error="";worker.received=false;
        worker.command=["lumen-window-geometry","--app",root.appId,"--title",request.title,"--container",String(request.container),"--outer-width",String(request.width),"--outer-height",String(request.height),"--minimum-width",String(root.ownerWindow.minimumSize.width),"--minimum-height",String(root.ownerWindow.minimumSize.height)];
        worker.running=true;
    }}
    Process {id:worker
        property bool received:false
        stdout:StdioCollector {onStreamFinished:{try{const response=JSON.parse(text);worker.received=true;if(response.error)root.error=response.error;else root.result=response;}catch(e){root.error="Invalid window geometry response";}}}
        onExited:(code,status)=>{root.completedRequests++;if(root.ownerWindow.visible && !received)root.error="Window geometry service did not respond";if(root.pending)drain.restart();}
    }
    Connections {target:root.ownerWindow
        function onVisibleChanged(){if(!root.ownerWindow.visible){root.pending=null;worker.running=false;}}
    }
}

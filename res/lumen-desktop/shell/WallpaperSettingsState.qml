import QtQuick
import Quickshell
import Quickshell.Io
Item {
    id:root
    property bool active:false
    property bool busy:worker.running
    property var entries:[]
    property string folder:""
    property string parentFolder:""
    property string browseError:""
    property bool truncated:false
    property var outputs:[]
    property var disconnected:[]
    property string error:""
    property var defaultMetadata:null
    property string defaultError:""
    property string defaultImage:""
    property var request:({action:"status"})
    function browse(path){send({action:"browse",path:path || null});}
    function refresh(){send({action:"status"});}
    function send(value){if(busy)return;request=value;error="";worker.running=true;}
    function choose(scope,identity,image){send({action:"set",scope:scope,identity:identity,image:image});}
    onActiveChanged:if(active)refresh()
    Component.onCompleted:if(active)refresh()
    Timer {interval:4000;repeat:true;running:root.active;onTriggered:root.refresh()}
    Process {
        id:worker;stdinEnabled:true
        command:[Quickshell.env("GLOBAL_MENU_PYTHON") || "python3",Qt.resolvedUrl("wallpaper_settings.py").toString().replace(/^file:\/\//,"")]
        onStarted:write(JSON.stringify(root.request)+"\n")
        stdout:StdioCollector {onStreamFinished:{try{const data=JSON.parse(text);if(data.action==="browse"){root.entries=data.entries || [];root.folder=data.folder || root.folder;root.parentFolder=data.parent || root.parentFolder;root.browseError=data.error || "";root.truncated=data.truncated===true;return;}if(data.outputs!==undefined)root.outputs=data.outputs;if(data.disconnected!==undefined)root.disconnected=data.disconnected;root.defaultImage=data.defaultImage || "";root.defaultMetadata=data.defaultMetadata || null;root.defaultError=data.defaultError || "";root.error=data.error || "";}catch(error){root.error="Unable to read background settings.";}}}
        onExited:(code,status)=>{if(code!==0)root.error="The background provider did not respond.";}
    }
}

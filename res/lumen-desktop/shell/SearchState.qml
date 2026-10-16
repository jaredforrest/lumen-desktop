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
    property int sequence: 0
    property int searchId: -1
    property int previewId: -1
    property int openId: -1
    signal opened()
    readonly property string python: Quickshell.env("GLOBAL_MENU_PYTHON") || "python3"
    readonly property string helper: Qt.resolvedUrl("search.py").toString().replace(/^file:\/\//, "")
    function send(action, data) {
        if (!available) return -1;
        const id=++sequence;
        transport.write(JSON.stringify(Object.assign({id:id,action:action},data || {}))+"\n");
        return id;
    }
    function search(text) {
        query=text;files=[];more=false;error="";searchId=-1;searching=text.trim().length>0;
        debounce.restart();
    }
    function requestPreview(path) {
        previewPath=path;preview={};previewId=-1;previewDelay.restart();
    }
    function open(path, parent) {
        if(opening || !available)return;
        error="";opening=true;openId=send(parent?"parent":"open",{path:path});
    }
    function refresh() { if(available)send("refresh"); }
    onActiveChanged: {
        if(active && !transport.running)transport.running=true;
        if(!active){previewPath="";preview={};previewId=-1;}
    }
    Timer {id:debounce;interval:140;onTriggered: {
        if(root.available && root.active)root.searchId=root.send("search",{query:root.query});
    }}
    Timer {id:previewDelay;interval:160;onTriggered: {
        if(root.available && root.previewPath)root.previewId=root.send("preview",{path:root.previewPath});
    }}
    Timer {id:indexRefresh;interval:400;onTriggered: {
        if(root.active && root.query.trim())root.searchId=root.send("search",{query:root.query});
    }}
    Process {
        id:transport
        command:[root.python,root.helper]
        stdinEnabled:true
        stdout:SplitParser {onRead:line=>{
            try {
                const data=JSON.parse(line);
                if(data.event==="ready" || data.event==="index") {
                    if(data.event==="ready"){root.available=true;debounce.restart();}
                    root.scanning=data.scanning;root.indexed=data.count;root.limited=data.limited;root.unreadable=data.unreadable;
                    if(root.active && root.query.trim())indexRefresh.restart();
                } else if(data.id===root.searchId) {
                    root.searching=false;root.error=data.error || "";root.files=data.results || [];root.more=data.more || false;
                } else if(data.id===root.previewId) {
                    root.preview=data.error?{type:"generic",text:data.error}:data;
                } else if(data.id===root.openId) {
                    root.opening=false;root.error=data.error || "";if(data.opened)root.opened();
                }
            } catch(e){root.error="Unable to read file search results";}
        }}
        onExited:(code,status)=>{
            root.available=false;root.searching=false;root.opening=false;
            if(root.active){root.error="File search disconnected. Reconnecting…";reconnect.restart();}
        }
    }
    Timer {id:reconnect;interval:2000;onTriggered:if(root.active)transport.running=true}
}

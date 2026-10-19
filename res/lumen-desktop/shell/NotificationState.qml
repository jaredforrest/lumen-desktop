import QtQuick
import Quickshell
import Quickshell.Io
Item {
    id: root
    required property var shell
    property double transportEpoch:0
    property string instance:""
    property double revision:0
    property double commandUntil:0
    function clearTransport(reset){available=false;rows=[];commandUntil=0;liveness.stop();if(reset===true){instance="";revision=0;}expandedCards=({});actionTimer.stop();}
    property bool available: false
    property bool dnd: false
    property var focusState: ({profiles:[],active:"",name:"",until:0,scheduled:false})
    property var policies:({})
    readonly property var applications:{
        let apps=Object.keys(policies).map(k=>({key:k,name:k}));
        for(const row of rows){const key=row.desktop || row.app;const found=apps.find(a=>a.key===key);if(found)found.name=row.app;else apps.push({key:key,name:row.app});}
        return apps.sort((a,b)=>a.name.localeCompare(b.name));
    }
    property var rows: []
    property string error: ""
    property var expandedCards: ({})
    function toggleExpanded(id){const next=Object.assign({},expandedCards);next[id]=!next[id];expandedCards=next;}
    property string bannerOutput: ""
    readonly property int unread: rows.filter(r=>r.unread && !r.transient).length
    readonly property var history: rows.filter(r=>!r.transient)
    readonly property var banners: rows.filter(r=>r.banner)
    readonly property var groups: {
        let result=[];
        for(const row of history){const key=row.desktop || row.app;let group=result.find(g=>g.key===key);if(!group){group={key:key,name:row.app,rows:[]};result.push(group);}group.rows.push(row);}
        return result;
    }
    function applyState(line) {
            try{const state=JSON.parse(line);if(!transport.running || state.transport!==String(root.transportEpoch))return;if(state.error){root.clearTransport();root.error=state.error;transport.running=false;return;}
                if(state.liveness===true){
                    if(root.available && root.instance && state.instance===root.instance && Number.isSafeInteger(state.commandUntil) && state.commandUntil>0){root.commandUntil=state.commandUntil;liveness.restart();}
                    return;
                }
                if(typeof state.instance!=="string" || !/^[0-9a-f]{32}$/.test(state.instance) || !Number.isSafeInteger(state.revision) || state.revision<=0 || !Array.isArray(state.rows) || !Number.isSafeInteger(state.commandUntil) || state.commandUntil<=0)throw new Error("Invalid notification transport");
                if(root.instance && root.instance!==state.instance){root.clearTransport();transport.running=false;return;}
                if(root.instance && state.revision<=root.revision)return;
                root.instance=state.instance;root.revision=state.revision;root.commandUntil=state.commandUntil;liveness.restart();if(state.previewsVisible!==true)actionTimer.stop();
                const old=root.rows.length?root.rows[0]:null;
                root.available=state.ready;root.dnd=state.dnd;root.focusState=state.focus || {profiles:[],active:"",name:"",until:0,scheduled:false};root.policies=state.policies || {};root.rows=state.rows;root.error="";
                if(state.rows.length && state.rows[0].banner && (!old || old.id!==state.rows[0].id || old.timestamp!==state.rows[0].timestamp))root.bannerOutput=root.shell.preferredOutput();
                if(Object.values(root.shell.bars).some(b=>b && b.menu==="Date"))root.seen();
            }catch(e){root.clearTransport();root.error="Unable to read notifications";transport.running=false;}
    }
    function command(data) {if(available)transport.write(JSON.stringify(Object.assign({instance:root.instance,expires:root.commandUntil},data))+"\n");}
    function seen(){if(rows.some(r=>r.unread || r.banner))command({action:"seen"});}
    function invoke(row,key){if(!available)return;shell.claimPopup("");actionTimer.data={action:"invoke",id:row.id,key:key,delivery:row.delivery,instance:root.instance,expires:root.commandUntil};actionTimer.restart();}
    Timer {id:actionTimer;property var data;interval:80;onTriggered:root.command(data)}
    Process {
        id:transport
        command:[Quickshell.env("GLOBAL_MENU_PYTHON") || "python3",Quickshell.env("LUMEN_NOTIFICATION_HELPER"),"watch",String(root.transportEpoch)]
        running:true;stdinEnabled:true
        stdout:SplitParser {onRead:line=>root.applyState(line)}
        onExited:(code,status)=>{root.clearTransport(true);reconnect.restart();}
    }
    Timer {id:liveness;interval:2500;onTriggered:{root.clearTransport();root.error="Notification service is unavailable";transport.running=false;}}
    Timer {id:reconnect;interval:2000;onTriggered:{root.transportEpoch+=1;transport.running=true;}}
}

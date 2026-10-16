import QtQuick
import Quickshell
import Quickshell.Io
Item {
    id: root
    required property var shell
    property bool available: false
    property bool dnd: false
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
    function command(data) {if(available)transport.write(JSON.stringify(data)+"\n");}
    function seen(){if(rows.some(r=>r.unread || r.banner))command({action:"seen"});}
    function invoke(row,key){shell.claimPopup("");actionTimer.data={action:"invoke",id:row.id,key:key,delivery:row.delivery};actionTimer.restart();}
    Timer {id:actionTimer;property var data;interval:80;onTriggered:root.command(data)}
    Process {
        id:transport
        command:[Quickshell.env("GLOBAL_MENU_PYTHON") || "python3",Quickshell.env("LUMEN_NOTIFICATION_HELPER"),"watch"]
        running:true;stdinEnabled:true
        stdout:SplitParser {onRead:line=>{
            try{const state=JSON.parse(line);if(state.error){root.error=state.error;return;}
                const old=root.rows.length?root.rows[0]:null;
                root.available=state.ready;root.dnd=state.dnd;root.policies=state.policies || {};root.rows=state.rows;root.error="";
                if(state.rows.length && state.rows[0].banner && (!old || old.id!==state.rows[0].id || old.timestamp!==state.rows[0].timestamp))root.bannerOutput=root.shell.preferredOutput();
                if(Object.values(root.shell.bars).some(b=>b && b.menu==="Date"))root.seen();
            }catch(e){root.error="Unable to read notifications";}
        }}
        onExited:(code,status)=>{root.available=false;reconnect.restart();}
    }
    Timer {id:reconnect;interval:2000;onTriggered:transport.running=true}
}

import QtQuick
import Quickshell
import Quickshell.Io
QtObject {
    id:root
    required property var service
    required property var shell
    property bool active:false
    property bool keyboardMode:false
    property var source:null
    property var target:null
    property string token:""
    property var destinations:[]
    property int selected:0
    property string error:""
    property int commits:0
    property var request:null
    readonly property var state:service && service.state?service.state:({available:false,enabled:false,locked:true,outputs:[]})
    signal began()
    function cancel(){active=false;token="";keyboardMode=false;source=null;target=null;destinations=[];}
    function begin(value,keyboard){
        if(worker.running || !value || !state.available || state.membershipSupported!==true || !state.enabled || state.locked || state.epoch!==value.epoch || state.revision!==value.revision)return false;
        const output=(state.outputs || []).find(o=>o.name===value.output && o.workspace===value.workspace && !o.suspended);
        if(!output || !output.groups.some(g=>g.id===value.sourceGroup))return false;
        source=Object.assign({},value);token=Date.now()+":"+Math.random();keyboardMode=keyboard;destinations=output.groups.filter(g=>g.id!==value.sourceGroup).map(g=>({kind:"group",groupId:g.id,label:"Group "+g.id})).concat([{kind:"new",label:"New Group"}]);selected=0;target=null;active=true;error="";began();return true;
    }
    function valid(){return active && !!source && state.available && state.membershipSupported===true && state.enabled && !state.locked && state.epoch===source.epoch && state.revision===source.revision && (state.outputs || []).some(o=>o.name===source.output && o.workspace===source.workspace && !o.suspended);}
    onStateChanged:if(active && !valid())cancel()
    function accepts(offer){return valid() && !keyboardMode && token!=="" && offer && offer.getDataAsString("application/x-lumen-group-intent")===token;}
    function preview(value){if(valid())target=Object.assign({},value);}
    function commit(value){
        if(!valid() || !value || !destinations.some(d=>d.kind===value.kind && (d.kind==="new" || d.groupId===value.groupId))){cancel();return;}
        request=Object.assign({},source,value);cancel();worker.running=true;
    }
    function step(delta){if(active && destinations.length)selected=(selected+delta+destinations.length)%destinations.length;}
    property Connections popupEvents:Connections {target:root.shell;function onDismissPopups(owner){if(root.active && owner!=="groups:"+root.source.output)root.cancel();}}
    property Process worker:Process {stdinEnabled:true
        command:["lumen-group-control","membership"]
        onStarted:write(JSON.stringify(root.request)+"\n")
        stdout:StdioCollector {onStreamFinished:{try{const result=JSON.parse(text);root.error=result.error || "";if(result.ok)root.commits++;}catch(error){root.error="Unable to change the captured group membership";}}}
    }
}

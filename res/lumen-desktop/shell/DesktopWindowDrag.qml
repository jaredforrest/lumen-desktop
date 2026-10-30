import QtQuick
import Quickshell.Io
QtObject {
    id:root
    required property var owner
    property bool active:false
    property bool keyboardMode:false
    property var source:null
    property var destinations:[]
    property int selected:0
    property string token:""
    property var request:null
    property string error:""
    property int commits:0
    property string observedGroupEpoch:""
    readonly property bool busy:worker.running
    readonly property var nativeState:owner.shell.windowGroupPresentation || null
    onNativeStateChanged:if(active && nativeState && (nativeState.locked===true || nativeState.available!==true || (observedGroupEpoch && nativeState.epoch!==observedGroupEpoch)))cancel()
    signal began()
    function cancel(){active=false;keyboardMode=false;source=null;destinations=[];token="";}
    function valid(){return active && !!source && owner.visible && !owner.busy && owner.sessionEpoch===source.sessionEpoch && owner.revision===source.revision;}
    function begin(value,keyboard){
        if(worker.running || active || !value || !owner.visible || owner.busy || (nativeState && (nativeState.locked!==false || nativeState.available!==true)))return false;
        const safe=id=>Number.isSafeInteger(id) && id>0;
        if(!Number.isSafeInteger(value.revision) || value.revision<0 || !value.source || ![value.source.id,value.source.pid,value.source.workspace,value.source.output].every(safe) || !Array.isArray(value.destinations) || !value.destinations.every(d=>safe(d.output) && (d.workspace===null || safe(d.workspace))))return false;
        source=JSON.parse(JSON.stringify(value));
        destinations=source.destinations;delete source.destinations;
        if(!destinations.length){source=null;return false;}
        observedGroupEpoch=nativeState?nativeState.epoch || "":"";
        keyboardMode=keyboard;selected=0;token=Date.now()+":"+Math.random();error="";active=true;began();return true;
    }
    function accepts(offer){return valid() && !keyboardMode && !!token && offer && offer.getDataAsString("application/x-lumen-desktop-window-intent")===token;}
    function canTarget(value){return valid() && value && destinations.some(d=>JSON.stringify(d)===JSON.stringify(value));}
    function commit(value){
        if(!canTarget(value)){cancel();return;}
        request=Object.assign({action:"transfer"},source,{destination:JSON.parse(JSON.stringify(value))});
        cancel();worker.running=true;
    }
    property Process worker:Process {
        property bool received:false
        property bool timedOut:false
        command:["lumen-window-overview"];stdinEnabled:true
        onStarted:{received=false;timedOut=false;write(JSON.stringify(root.request)+"\n");deadline.restart();}
        stdout:StdioCollector {onStreamFinished:{
            if(worker.timedOut)return;worker.received=true;
            try{const reply=JSON.parse(text);root.error=reply.ok===true?"":reply.error || "The captured desktop transfer was rejected";if(reply.ok===true)root.commits++;}
            catch(error){root.error="The desktop transfer did not return a valid confirmation";}
        }}
        onExited:{deadline.stop();killDelay.stop();if(!received && !timedOut)root.error="The desktop transfer did not confirm its result. Refresh before trying again.";}
    }
    property Timer deadline:Timer {interval:15000;onTriggered:{worker.timedOut=true;root.error="The desktop transfer confirmation timed out. Refresh before trying again.";worker.signal(15);killDelay.restart();}}
    property Timer killDelay:Timer {interval:500;onTriggered:if(worker.running)worker.signal(9)}
}

import QtQuick
import Quickshell
import Quickshell.Io
QtObject {
    id:root
    property var state:({available:false,enabled:false,locked:true,outputs:[]})
    readonly property string desiredMode:Theme.windowsPreferences.groupWindowMode || "all_together"
    property string presentationError:""
    property string lastModeAttempt:""
    property var modeRequest:null
    function reconcileMode(){
        if(modeWorker.running || state.available!==true || state.presentationSupported!==true || state.locked!==false)return;
        if(!["all_together","one_at_a_time"].includes(desiredMode) || !["all_together","one_at_a_time"].includes(state.windowMode))return;
        if(state.windowMode===desiredMode){presentationError="";return;}
        const captured={epoch:state.epoch,revision:state.revision,windowMode:desiredMode};
        const signature=JSON.stringify(captured);
        if(signature===lastModeAttempt)return;
        lastModeAttempt=signature;modeRequest=captured;modeWorker.running=true;
    }
    onDesiredModeChanged:modeApply.restart()
    onStateChanged:modeApply.restart()
    function unavailable(){state={available:false,enabled:false,locked:true,outputs:[]};}
    property Process observer:Process {
        running:true
        command:["lumen-group-control","watch"]
        stdout:SplitParser {onRead:data=>{try{const value=JSON.parse(data);root.state=value;expiry.restart();}catch(error){root.unavailable();}}}
        onExited:{root.unavailable();retry.restart();}
    }
    property Timer expiry:Timer {interval:2200;onTriggered:root.unavailable()}
    property Timer retry:Timer {interval:3000;onTriggered:root.observer.running=true}
    property Timer modeApply:Timer {interval:100;onTriggered:root.reconcileMode()}
    property Timer modeDeadline:Timer {interval:15000;onTriggered:{modeWorker.timedOut=true;root.presentationError="Group presentation confirmation timed out. The current native state will be observed before another attempt.";modeWorker.signal(15);modeKill.restart();}}
    property Timer modeKill:Timer {interval:500;onTriggered:if(modeWorker.running)modeWorker.signal(9)}
    property Process modeWorker:Process {
        property bool received:false
        property bool timedOut:false
        command:["lumen-group-control","mode"]
        stdinEnabled:true
        onStarted:{received=false;timedOut=false;write(JSON.stringify(root.modeRequest)+"\n");modeDeadline.restart();}
        stdout:StdioCollector {onStreamFinished:{
            if(modeWorker.timedOut)return;
            try{const result=JSON.parse(text);modeWorker.received=true;root.presentationError=result.ok===true?"":result.error || "Unable to apply group presentation";}
            catch(error){root.presentationError="Invalid group presentation confirmation";}
        }}
        onExited:{modeDeadline.stop();modeKill.stop();if(!received && !timedOut)root.presentationError="Group presentation service did not confirm the change";modeApply.restart();}
    }
}

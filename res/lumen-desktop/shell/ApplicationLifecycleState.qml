import QtQuick
import Quickshell
import Quickshell.Io

QtObject {
    id:root
    property bool available:false
    property bool recoveryAvailable:false
    property var applications:[]
    property var activationHistory:[]
    property var nativeWindows:[]
    property var nativeObservation:null
    property bool nativeCoverageComplete:false
    property bool nativeGuardAvailable:false
    property var nativeFocused:null
    property int nativeSerial:0
    property bool nativePending:false
    property string nativeRequest:""
    property bool nativeRetiring:false
    property bool nativeAwaitingExit:false
    property bool nativeRestartStarted:false
    signal nativeFocusResult(var result)
    function clearObservation(){
        recoveryAvailable=false;available=false;applications=[];activationHistory=[];
        nativeObservation=null;nativeWindows=[];nativeCoverageComplete=false;nativeGuardAvailable=false;nativeFocused=null;sessionEpoch="";pending=false;
    }
    function cancelNative(reason,retire){
        const request=nativeRequest;
        nativeDeadline.stop();nativeRequest="";nativePending=false;
        if(retire){
            nativeRetiring=true;nativeRestartStarted=false;nativeAwaitingExit=observer.running;
            clearObservation();observer.running=false;
        }
        if(request)nativeFocusResult({requestId:request,requested:false,uncertain:true,error:reason});
    }
    function focusNative(capture,initialFocus,switchSpace,inventoryWitness){
        if(!available || nativePending || nativeRetiring || nativeAwaitingExit || !observer.running)return "";
        const request=sessionEpoch+":"+(++nativeSerial);
        nativeRequest=request;nativePending=true;nativeDeadline.restart();
        try{observer.write(JSON.stringify({action:"focus_native",capture:capture,initialFocus:initialFocus,inventoryWitness:inventoryWitness,switchSpace:switchSpace,requestId:request})+"\n");}
        catch(error){cancelNative("The window request was interrupted. Its result is uncertain; wait for the desktop to reconnect.",true);return "";}
        return request;
    }
    // Two bounded compositor write/read stages can take twenty seconds. The
    // total budget also covers validation; a timeout retires this owned worker.
    property Timer nativeDeadline:Timer {interval:30000;onTriggered:root.cancelNative("The window request did not finish. Its result is uncertain; wait for the desktop to reconnect.",true)}
    property string sessionEpoch:""
    property string error:""
    property bool pending:false
    property var session:({phase:"idle",token:"",error:""})
    signal sessionAuthorized(var authorization)
    signal resultReady(var result)
    function sessionCommand(action,value){if(!available)return false;observer.write(JSON.stringify({action:action,sessionAction:value,token:value})+"\n");return true;}
    function quit(appId,instanceIds){
        if(!available || pending)return false;
        pending=true;observer.write(JSON.stringify({action:"quit",appId:appId,instanceIds:instanceIds || ((applications.find(app=>app.appId===appId) || {}).instanceIds) || []})+"\n");return true;
    }
    property Process observer:Process {
        running:true;stdinEnabled:true
        onStarted:{if(root.nativeRetiring && !root.nativeAwaitingExit)root.nativeRestartStarted=true;write(JSON.stringify({action:"status"})+"\n");}
        command:["lumen-application-lifecycle"]
        stderr:StdioCollector {onStreamFinished:if(text)root.error="Application observation failed. Window-based controls remain available."}
        stdout:SplitParser {onRead:data=>{
            // Late output from a retiring worker cannot revive its availability
            // or acknowledge a newer invocation. onExited is the reap boundary.
            if(root.nativeAwaitingExit || (root.nativeRetiring && !root.nativeRestartStarted))return;
            try {
                const value=JSON.parse(data);
                if(!value || typeof value!=="object" || Array.isArray(value) || typeof value.available!=="boolean")throw Error("Invalid observation response");
                if(value.available!==true && root.nativePending){root.cancelNative("The desktop disconnected during the window request. Its result is uncertain; wait for reconnection.",true);return;}
                root.available=value.available===true;root.recoveryAvailable=root.available && value.recoveryAvailable===true;root.applications=root.available && Array.isArray(value.applications)?value.applications:[];root.error=value.error || "";root.session=value.session || {phase:"idle",token:"",error:""};
                root.activationHistory=root.available && Array.isArray(value.activationHistory)?value.activationHistory:[];root.sessionEpoch=root.available && typeof value.sessionEpoch==="string"?value.sessionEpoch:"";
                root.nativeGuardAvailable=root.available && value.nativeGuardAvailable===true;root.nativeCoverageComplete=root.available && value.nativeCoverageComplete===true;root.nativeWindows=root.available && Array.isArray(value.nativeWindows)?value.nativeWindows:[];root.nativeFocused=root.available?value.nativeFocused || null:null;
                root.nativeObservation=root.available && root.nativeGuardAvailable && value.nativeObservation && value.nativeObservation.version===1 && value.nativeObservation.epoch===root.sessionEpoch?value.nativeObservation:null;
                if(root.available)root.nativeRetiring=false;
                if(value.result){
                    if(value.result.nativeFocus){
                        const result=value.result.nativeFocus;
                        if(!root.nativePending)return;
                        if(result.requestId!==root.nativeRequest || typeof result.requested!=="boolean"){
                            root.cancelNative("The desktop returned an unexpected window response. Its result is uncertain; wait for reconnection.",true);return;
                        }
                        root.nativeDeadline.stop();root.nativePending=false;root.nativeRequest="";root.nativeFocusResult(result);
                    }else{root.pending=false;if(value.result.sessionAuthorization)root.sessionAuthorized(value.result.sessionAuthorization);else root.resultReady(value.result);}
                }
            }catch(error){
                root.cancelNative("The desktop response was interrupted. The window request may have completed; wait for reconnection.",true);
                root.clearObservation();
            }
        }}
        onExited:{
            if(root.nativePending)root.nativeRetiring=true;
            root.cancelNative("The desktop disconnected during the window request. Its result is uncertain; wait for reconnection.",false);
            root.nativeAwaitingExit=false;root.nativeRestartStarted=false;root.clearObservation();
            root.session={phase:"idle",token:"",error:""};retry.restart();
        }
    }
    property Timer retry:Timer {interval:3000;onTriggered:root.observer.running=true}
}

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
    property var nativeFocused:null
    property int nativeSerial:0
    property bool nativePending:false
    signal nativeFocusResult(var result)
    function focusNative(capture,initialFocus,switchSpace){
        if(!available || nativePending)return "";
        const request=sessionEpoch+":"+(++nativeSerial);nativePending=true;
        observer.write(JSON.stringify({action:"focus_native",capture:capture,initialFocus:initialFocus,switchSpace:switchSpace,requestId:request})+"\n");return request;
    }
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
        onStarted:write(JSON.stringify({action:"status"})+"\n")
        command:["lumen-application-lifecycle"]
        stderr:StdioCollector {onStreamFinished:if(text)root.error="Application observation failed. Window-based controls remain available."}
        stdout:SplitParser {onRead:data=>{
            try {
                const value=JSON.parse(data);
                root.available=value.available===true;root.recoveryAvailable=root.available && value.recoveryAvailable===true;root.applications=root.available && Array.isArray(value.applications)?value.applications:[];root.error=value.error || "";root.session=value.session || {phase:"idle",token:"",error:""};
                root.activationHistory=root.available && Array.isArray(value.activationHistory)?value.activationHistory:[];root.sessionEpoch=root.available && typeof value.sessionEpoch==="string"?value.sessionEpoch:"";
                root.nativeWindows=root.available && Array.isArray(value.nativeWindows)?value.nativeWindows:[];root.nativeFocused=root.available?value.nativeFocused || null:null;
                if(value.result){if(value.result.nativeFocus){root.nativePending=false;root.nativeFocusResult(value.result.nativeFocus);}else{root.pending=false;if(value.result.sessionAuthorization)root.sessionAuthorized(value.result.sessionAuthorization);else root.resultReady(value.result);}}
            }catch(error){root.recoveryAvailable=false;root.available=false;root.applications=[];root.activationHistory=[];root.nativeWindows=[];root.nativeFocused=null;root.nativePending=false;root.sessionEpoch="";root.pending=false;}
        }}
        onExited:{root.recoveryAvailable=false;root.available=false;root.applications=[];root.activationHistory=[];root.nativeWindows=[];root.nativeFocused=null;root.nativePending=false;root.nativeFocusResult({requested:false,requestId:"",error:"Native observation restarted"});root.sessionEpoch="";root.pending=false;root.session={phase:"idle",token:"",error:""};retry.restart();}
    }
    property Timer retry:Timer {interval:3000;onTriggered:root.observer.running=true}
}

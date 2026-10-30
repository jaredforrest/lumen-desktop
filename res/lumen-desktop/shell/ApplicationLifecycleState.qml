import QtQuick
import Quickshell
import Quickshell.Io

QtObject {
    id:root
    property bool available:false
    property bool recoveryAvailable:false
    property var applications:[]
    property var activationHistory:[]
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
                if(value.result){root.pending=false;if(value.result.sessionAuthorization)root.sessionAuthorized(value.result.sessionAuthorization);else root.resultReady(value.result);}
            }catch(error){root.recoveryAvailable=false;root.available=false;root.applications=[];root.activationHistory=[];root.sessionEpoch="";root.pending=false;}
        }}
        onExited:{root.recoveryAvailable=false;root.available=false;root.applications=[];root.activationHistory=[];root.sessionEpoch="";root.pending=false;root.session={phase:"idle",token:"",error:""};retry.restart();}
    }
    property Timer retry:Timer {interval:3000;onTriggered:root.observer.running=true}
}

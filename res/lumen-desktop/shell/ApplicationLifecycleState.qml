import QtQuick
import Quickshell
import Quickshell.Io

QtObject {
    id:root
    property bool available:false
    property bool recoveryAvailable:false
    property var applications:[]
    property string error:""
    property bool pending:false
    signal resultReady(var result)
    function quit(appId){
        if(!available || pending)return false;
        pending=true;observer.write(JSON.stringify({action:"quit",appId:appId})+"\n");return true;
    }
    property Process observer:Process {
        running:true;stdinEnabled:true
        onStarted:write(JSON.stringify({action:"status"})+"\n")
        command:["lumen-application-lifecycle"]
        stderr:StdioCollector {onStreamFinished:if(text)root.error="Application observation failed. Window-based controls remain available."}
        stdout:SplitParser {onRead:data=>{
            try {
                const value=JSON.parse(data);
                root.available=value.available===true;root.recoveryAvailable=root.available && value.recoveryAvailable===true;root.applications=root.available && Array.isArray(value.applications)?value.applications:[];root.error=value.error || "";
                if(value.result){root.pending=false;root.resultReady(value.result);}
            }catch(error){root.recoveryAvailable=false;root.available=false;root.applications=[];root.pending=false;}
        }}
        onExited:{root.recoveryAvailable=false;root.available=false;root.applications=[];root.pending=false;retry.restart();}
    }
    property Timer retry:Timer {interval:3000;onTriggered:root.observer.running=true}
}

import QtQuick
import Quickshell
import Quickshell.Io

Item {
    id: root
    property var mullvad: ({installed:false,state:"unavailable",account:false,location:"",relay:""})
    property var tailscale: ({installed:false,state:"unavailable",online:false,peers:0,exitNode:"",exitNodes:[],health:[]})
    property string error: ""
    property bool busy: worker.running
    property var request: ({action:"status"})
    readonly property bool anyConnected: mullvad.state === "connected" || tailscale.online
    function refresh() {
        if (worker.running) return;
        request={action:"status"};worker.running=true;
    }
    function run(action, values) {
        if (worker.running) return;
        error="";request=Object.assign({action:action},values || {});worker.running=true;
    }
    Component.onCompleted:refresh()
    Process {
        id: worker
        stdinEnabled: true
        command: [Quickshell.env("GLOBAL_MENU_PYTHON") || "python3", Qt.resolvedUrl("vpn_control.py").toString().replace("file://", "")]
        onStarted:write(JSON.stringify(root.request)+"\n")
        stdout:StdioCollector {onStreamFinished:{
            try {
                const result=JSON.parse(text);
                if(result.error){root.error=result.error;return;}
                root.mullvad=result.mullvad;root.tailscale=result.tailscale;root.error="";
            } catch(e) {root.error="Could not read VPN status";}
        }}
        onExited:(code,status)=>{if(code!==0 && !root.error)root.error="VPN service did not respond";}
    }
    Timer {interval:10000;running:true;repeat:true;onTriggered:root.refresh()}
}

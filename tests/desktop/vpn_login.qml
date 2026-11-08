import QtQuick
import Quickshell
import "LUMEN_ASSETS" as D
ShellRoot {
    property int step: 0
    QtObject {
        id: service
        property var mullvad: ({installed:true,state:"disconnected",account:false,expires:"",relay:""})
        property var tailscale: ({installed:true,state:"NeedsLogin",online:false,exitNodes:[],health:[]})
        property bool busy: false
        property string error: ""
        property string action: ""
        function run(value) { action=value; }
    }
    FloatingWindow {
        id: window
        visible: true
        implicitWidth: 560
        implicitHeight: 600
        D.VpnPanel { id: panel; x:20; y:20; width:window.width-40; service:service }
    }
    function find(item,name) {
        if(item.objectName===name)return item;
        for(const child of item.children || []){const found=find(child,name);if(found)return found;}
        return null;
    }
    Timer {
        interval:150; running:true; repeat:true
        onTriggered: {
            try {
                const button=find(panel,"tailscaleSignIn");
                if(!button)throw Error("Missing Tailscale sign-in button");
                if(step===0){
                    if(!button.visible || !button.enabled)throw Error("Sign in unavailable while logged out");
                    button.clicked();
                    if(service.action!=="tailscale-login")throw Error("Wrong sign-in action");
                    service.busy=true;
                }else if(step===1){
                    if(button.enabled)throw Error("Duplicate sign-in allowed while busy");
                    service.busy=false;panel.compact=true;window.implicitWidth=340;
                }else if(step===2){
                    if(!button.visible)throw Error("Compact menu missing Sign In");
                    service.tailscale=({installed:true,state:"Running",online:true,exitNodes:[],health:[]});
                }else if(step===3){
                    if(button.visible)throw Error("Sign-in button visible after login");
                    console.log("VPN_LOGIN_PASS");Qt.quit();
                }
                step++;
            }catch(e){console.error("VPN_LOGIN_FAIL "+e);Qt.quit();}
        }
    }
}

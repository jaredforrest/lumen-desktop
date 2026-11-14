import QtQuick
import QtQuick.Window
import Quickshell
import "LUMEN_ASSETS" as D
ShellRoot {
    QtObject {
        id:controls
        property var wifi:({available:true,enabled:true,backend:"network-manager",phase:"connected",name:"Fixture",connectivity:"portal",networks:[]})
        property bool busy:false
        property string pendingAction:""
        property string error:""
        property var calls:[]
        function act(action,id,password){calls.push([action,id,password]);}
    }
    Window {visible:true;width:360;height:550
        D.WifiPane {id:pane;width:330;controls:controls}
    }
    function find(item,name){
        if(item.objectName===name)return item;
        for(const child of item.children || []){const match=find(child,name);if(match)return match;}
        return null;
    }
    Timer {interval:100;running:true;onTriggered:{
        try {
            const toggle=find(pane,"wifi-power");
            if(!toggle || !toggle.checked || !toggle.enabled)throw Error("Wi-Fi toggle is not usable");
            const refresh=find(pane,"wifi-refresh");
            if(!refresh || refresh.height!==30 || refresh.contentItem.horizontalAlignment!==Text.AlignHCenter || refresh.contentItem.verticalAlignment!==Text.AlignVCenter)throw Error("Refresh label is not centered");
            pane.connect({id:"11:22:33:44:55:66",name:"Open",secured:false});
            if(controls.calls.length!==1 || controls.calls[0][0]!=="wifi-connect")throw Error("Open network did not connect");
            pane.connect({id:"AA:BB:CC:DD:EE:FF",name:"Private",secured:true});
            if(controls.calls.length!==1 || pane.joining.name!=="Private")throw Error("Secured network skipped password confirmation");
            pane.connect({id:"11:22:33:44:55:66",name:"Already connected",connected:true});
            if(controls.calls.length!==1)throw Error("Current connection was unnecessarily reconnected");
            controls.busy=true;pane.connect({id:"11:22:33:44:55:66",name:"Open",secured:false});
            if(controls.calls.length!==1)throw Error("Concurrent connection was allowed");
            console.log("PASS Wi-Fi connection and password interaction");Qt.quit();
        } catch(e){console.log("FAIL "+e);Qt.exit(1);}
    }}
}

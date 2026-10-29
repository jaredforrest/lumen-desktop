import QtQuick
import Quickshell
import Quickshell.Io
import "DisplaySpacesModel.js" as Model
QtObject {
    id:root
    property var state:({available:false,applied:false,separateDisplays:true,primary:"",primaryDisplay:null,outputs:[],error:"Display policy is unavailable"})
    property var routing:({mode:"independent",primary:""})
    property var lastAccepted:null
    function unavailable(){state={available:false,applied:false,separateDisplays:true,primary:"",primaryDisplay:null,outputs:[],error:"Display policy observer is unavailable; retained Dock routing remains usable"};}
    function accept(value){if(!Model.valid(value,lastAccepted)){unavailable();return false;}lastAccepted=value;routing=Model.routing(routing,value);if(JSON.stringify(value)!==JSON.stringify(state))state=value;expiry.restart();return true;}
    function dockOutput(requested){return Model.destination(routing,requested,Quickshell.screens);}
    function dockVisible(output){return Model.visible(routing,output,Quickshell.screens);}
    property Process observer:Process {
        running:true
        command:[Quickshell.env("LUMEN_DISPLAY_SPACES_BIN") || "lumen-display-spaces"]
        stdout:SplitParser {onRead:data=>{try{if(data.length>262144){root.unavailable();return;}root.accept(JSON.parse(data));}catch(error){root.unavailable();}}}
        onExited:{root.unavailable();retry.restart();}
    }
    property Timer expiry:Timer {interval:2200;onTriggered:root.unavailable()}
    property Timer retry:Timer {interval:3000;onTriggered:root.observer.running=true}
}

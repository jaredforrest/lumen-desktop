import QtQuick
import Quickshell
import Quickshell.Io
QtObject {
    id:root
    property bool available:false
    property var outputs:({})
    property string error:""
    property int transport:0
    property int revision:0
    function receive(line){try{
        const value=JSON.parse(line);
        if(value.transport!==String(root.transport) || !Number.isInteger(value.revision) || value.revision<=root.revision)return;
        if(!value.outputs || Array.isArray(value.outputs) || Object.values(value.outputs).some(state=>typeof state!=="boolean"))throw Error("Invalid output state");
        root.revision=value.revision;root.available=value.available===true;root.outputs=root.available?value.outputs:{};root.error=value.error || "";
    }catch(error){root.available=false;root.outputs={};root.error="Fullscreen menu observation is unavailable";}}
    property Process observer:Process {
        running:true
        command:["lumen-menu-visibility",String(root.transport)]
        stdout:SplitParser {onRead:line=>root.receive(line)}
        onExited:{root.available=false;root.outputs={};retry.restart();}
    }
    property Timer retry:Timer {interval:2000;onTriggered:{root.transport++;root.revision=0;root.observer.running=true;}}
}

import QtQuick
import QtQuick.Controls
import Quickshell.Io
Column {
    id:root
    required property var node
    width:parent.width
    spacing:5
    readonly property string label:node?(node.nickname || node.description || node.name):"Unavailable"
    readonly property bool available:!!node && !!node.audio && Number.isFinite(node.audio.volume)
    property real pendingVolume:-1
    property real optimisticVolume:-1
    property var pendingMute:null
    property string error:""
    property string writingNode:""
    function setVolume(value){if(!available)return;optimisticVolume=Math.max(0,Math.min(1,value));pendingVolume=optimisticVolume;error="";debounce.restart();}
    function toggleMute(){if(!available)return;pendingMute=!(pendingMute===null?node.audio.muted:pendingMute);flush();}
    function flush(){
        if(writer.running || !available)return;
        if(pendingMute!==null){writer.command=["wpctl","set-mute",String(node.id),pendingMute?"1":"0"];pendingMute=null;}
        else if(pendingVolume>=0){writer.command=["wpctl","set-volume","-l","1",String(node.id),String(pendingVolume)];pendingVolume=-1;}
        else return;
        writingNode=String(node.id);writer.running=true;
    }
    onNodeChanged:{debounce.stop();reconcile.stop();pendingVolume=-1;optimisticVolume=-1;pendingMute=null;error="";}
    PanelLabel {width:parent.width;text:root.label;textFormat:Text.PlainText;wrapMode:Text.Wrap;font.pixelSize:Theme.textRole("label")}
    Row {width:parent.width;spacing:8
        SystemSlider {
            id:level;width:parent.width-mute.width-8;height:mute.height;from:0;to:1;stepSize:0.01
            value:root.optimisticVolume>=0?root.optimisticVolume:root.available?root.node.audio.volume:0
            enabled:root.available;Accessible.name:root.label+" volume"
            onMoved:root.setVolume(value)
            onPressedChanged:if(!pressed)root.flush()

        }
        DialogButton {id:mute;width:70+(Theme.textRole("body")-13)*5;height:26+(Theme.textRole("body")-13)*2;text:root.available && root.node.audio.muted?"Unmute":"Mute";enabled:root.available;Accessible.name:text+" "+root.label;onClicked:root.toggleMute()}
    }
    PanelLabel {visible:!!root.error;width:parent.width;text:root.error;font.pixelSize:Theme.textRole("metadata");color:Theme.colors.muted;wrapMode:Text.WordWrap}
    Timer {id:debounce;interval:60;onTriggered:root.flush()}
    Timer {id:reconcile;interval:200;onTriggered:if(!writer.running && root.pendingVolume<0)root.optimisticVolume=-1}
    // WirePlumber also handles nodes whose channel map is not exposed to Quickshell.
    // Serialize/coalesce writes; PipeWire events remain the authoritative read state.
    Process {id:writer;onExited:(code,status)=>{if(root.node && String(root.node.id)===root.writingNode && code!==0)root.error="The sound setting could not be changed.";reconcile.restart();Qt.callLater(root.flush);}}
}

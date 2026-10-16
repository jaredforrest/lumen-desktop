import QtQuick
import QtQuick.Controls
import Quickshell
import Quickshell.Io
Column {
    id: root
    property var displays:[]
    property string selectedBus:""
    property var display:displays.find(d=>d.bus===selectedBus) || (displays.length===1?displays[0]:null)
    property real level: display ? display.level : 0
    property real pending: -1
    property string error: ""
    property bool editing: false
    property int revision: 0
    readonly property string python: Quickshell.env("GLOBAL_MENU_PYTHON") || "python3"
    readonly property string helper: Qt.resolvedUrl("system-controls.py").toString().replace(/^file:\/\//, "")
    spacing:6
    function flush() {
        if(writer.running || !display || pending<0)return;
        writer.command=[python,helper,"brightness",display.bus,String(Math.round(pending*display.maximum))];
        pending=-1;writer.running=true;
    }
    DesktopComboBox {visible:root.displays.length>1;width:parent.width;model:root.displays;textRole:"name";currentIndex:root.displays.findIndex(d=>d.bus===root.selectedBus);displayText:currentIndex<0?"Choose display":currentText;onActivated:index=>{root.selectedBus=root.displays[index].bus;root.level=root.display.level;} Accessible.name:"Brightness display"}
    PanelLabel {text:root.display?"Display":"Display · Unavailable";font.pixelSize:12;font.weight:Font.DemiBold}
    SystemSlider {
        id: slider;width:parent.width;height:18;from:0;to:1;value:root.level;enabled:!!root.display
        leftPadding:24;rightPadding:26
        Accessible.name:"Display brightness"
        leadingGlyph:"sun";trailingGlyph:"sun"
        onPressedChanged:{root.editing=pressed;if(!pressed)root.flush();}
        onMoved:{root.level=value;root.pending=value;root.revision++;root.error="";debounce.restart();}
        ToolTip {
            id: brightnessTip
            parent: slider
            visible: slider.hovered && !slider.pressed
            delay: 700
            timeout: 4000
            text: root.error || (root.display ? root.display.name : "Brightness control is not available for this display")
            x: Math.max(0, (slider.width-width)/2)
            y: slider.height+8
            padding: 7
            contentItem: Text {
                text: brightnessTip.text
                font.family: Theme.font
                font.pixelSize: 11
                color: Theme.colors.text
                textFormat: Text.PlainText
            }
            background: Rectangle {
                radius: 6
                color: Theme.colors.background
                border.color: Theme.outline
                border.width: 1
            }
        }
    }
    Timer {id:debounce;interval:100;onTriggered:root.flush()}
    Process {id:writer;onExited:(code,status)=>{if(code!==0)root.error="The display rejected the brightness change";if(root.pending>=0)root.flush();else refresh.restart();}}
    Process {
        id:reader;property int requestRevision:0
        command:[root.python,root.helper,"displays"]
        stdout:StdioCollector {onStreamFinished:{try{const s=JSON.parse(text);if(reader.requestRevision!==root.revision || root.editing || writer.running || root.pending>=0)return;root.displays=s.displays || [];if(root.display)root.level=root.display.level;}catch(e){root.displays=[];}}}
    }
    // Hidden panels do not poll hardware. Detection/read caches are shared by all controls.
    Timer {id:refresh;interval:30000;running:root.visible;repeat:true;triggeredOnStart:true;onTriggered:{if(!reader.running && !writer.running && !root.editing && root.pending<0){reader.requestRevision=root.revision;reader.running=true;}}}
}

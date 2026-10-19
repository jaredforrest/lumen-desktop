import QtQuick
import QtQuick.Controls
import Quickshell
import Quickshell.Io
import Quickshell.Wayland

PanelWindow {
    id: root
    required property var shell
    visible: false
    anchors { top: true; bottom: true; left: true; right: true }
    exclusionMode: ExclusionMode.Ignore
    color: "transparent"
    WlrLayershell.namespace: "lumen-window-layout"
    WlrLayershell.layer: WlrLayer.Overlay
    WlrLayershell.keyboardFocus: visible ? WlrKeyboardFocus.Exclusive : WlrKeyboardFocus.None
    property bool supported: false
    property bool showDesktopSupported: false
    property bool edgeTilingSupported: false
    property bool pairedLayoutsSupported:false
    property double target: 0
    property real anchorX: 0
    property real anchorY: 0
    property int selected: 0
    readonly property bool overTrigger: background.containsMouse && Math.abs(background.mouseX-(anchorX-(screen?screen.x:0)))<=14 && Math.abs(background.mouseY-(anchorY-(screen?screen.y:0)))<=14
    readonly property var choices: [
        {label:"Enter Full Screen",action:"fullscreen"},
        {label:"Fill",action:"fill"},
        {label:"Left Half",action:"left"},{label:"Right Half",action:"right"},
        {label:"Top Half",action:"top"},{label:"Bottom Half",action:"bottom"},
        {label:"Top Left",action:"top-left"},{label:"Top Right",action:"top-right"},
        {label:"Bottom Left",action:"bottom-left"},{label:"Bottom Right",action:"bottom-right"},
        {label:"Restore Previous Size",action:"restore"}
    ].concat(pairedLayoutsSupported?[
        {label:"Pair Full Screen on Left",action:"pair-left"},
        {label:"Pair Full Screen on Right",action:"pair-right"},
        {label:"Manage Paired Windows",action:"pair-manage"}
    ]:[])
    function dismiss() { opening.stop(); leaving.stop();visible=false;target=0;if(shell.popupOwner==="window-layout")shell.popupOwner=""; }
    function event(payload) {
        let data;
        try {data=JSON.parse(payload);}catch(e){return;}
        if(!data || typeof data!=="object" || Array.isArray(data))return;
        if(data.lumen_drag_overview===1){if(edgeTilingSupported && Number.isSafeInteger(data.id) && data.id>0)root.shell.dragOverview(data.id);return;}
        if(data.lumen_layout!==1 || !Number.isSafeInteger(data.id) || data.id<0)return;
        if(data.id===0){opening.stop();if(visible)leaving.restart();return;}
        if(!Number.isFinite(data.x) || !Number.isFinite(data.y))return;
        if(visible && target===data.id)return;
        target=data.id;anchorX=data.x;anchorY=data.y;opening.restart();
    }
    function open() {
        if(!target)return;
        screen=Quickshell.screens.find(s=>anchorX>=s.x && anchorX<s.x+s.width && anchorY>=s.y && anchorY<s.y+s.height) || Quickshell.screens[0];
        if(!screen)return;
        shell.claimPopup("window-layout");selected=0;visible=true;
        Qt.callLater(()=>keys.forceActiveFocus());
    }
    function apply(index) {
        if(!supported || index<0 || index>=choices.length || !Number.isSafeInteger(target) || target<=0)return;
        const id=target,action=choices[index].action;
        dismiss();
        if(action.startsWith("pair-")){root.shell.pairWindow(id,action.substring(5));return;}
        Quickshell.execDetached(["swaymsg","[con_id="+id+"] lumen_window "+action]);
    }
    function updateCapabilities(raw) {
        try {
            const version=JSON.parse(raw);
            supported=version.native_window_layouts===true;
            showDesktopSupported=version.native_show_desktop===true;
            edgeTilingSupported=version.native_edge_tiling===true;
            pairedLayoutsSupported=version.native_paired_layouts===true;
        } catch(error) { supported=false;showDesktopSupported=false;edgeTilingSupported=false;pairedLayoutsSupported=false; }
    }
    Process {
        running:!!Quickshell.env("SWAYSOCK")
        command:["swaymsg","-r","-t","get_version"]
        stdout:StdioCollector {onStreamFinished:root.updateCapabilities(text)}
    }
    Timer { id:opening; interval:450; onTriggered:root.open() }
    Timer { id:leaving; interval:320; repeat:true; onTriggered:if(!cardHover.hovered && !root.overTrigger)root.dismiss() }
    Connections { target:root.shell; function onDismissPopups(owner){if(owner!=="window-layout")root.dismiss();} }
    Timer {id:restart;interval:1000;onTriggered:events.running=!!Quickshell.env("SWAYSOCK")}
    Process {
        id:events
        running:!!Quickshell.env("SWAYSOCK")
        command:["swaymsg","-m","-t","subscribe","[\"tick\"]"]
        stdout:SplitParser {onRead:data=>{try {const message=JSON.parse(data);if(typeof message.payload==="string")root.event(message.payload);}catch(e){}}}
        onExited:restart.restart()
    }
    MouseArea {id:background;anchors.fill:parent;hoverEnabled:true;onClicked:root.dismiss()}
    Rectangle {
        id:card
        x:Math.max(8,Math.min(root.width-width-8,root.anchorX-(root.screen?root.screen.x:0)-52))
        y:Math.max(8,Math.min(root.height-height-8,root.anchorY-(root.screen?root.screen.y:0)+16))
        width:230;height:column.implicitHeight+12;radius:Theme.menuRadius
        color:Theme.popoverMaterial;border.color:Theme.outline
        HoverHandler {id:cardHover}
        Column {
            id:column;x:6;y:6;width:parent.width-12
            Text {visible:!root.supported;width:column.width;padding:8;wrapMode:Text.Wrap;text:"Window layouts become available after signing in to the updated desktop.";color:Theme.colors.text;font.family:Theme.font;font.pixelSize:12}
            Repeater {
                model:root.choices
                Button {
                    required property var modelData
                    required property int index
                    width:column.width;height:28
                    enabled:root.supported
                    Accessible.name:modelData.label
                    text:modelData.label
                    onHoveredChanged:if(hovered)root.selected=index
                    onClicked:root.apply(index)
                    background:Rectangle {radius:5;color:root.supported && root.selected===index?Theme.colors.accent:"transparent"}
                    contentItem:Text {text:parent.text;color:root.supported && root.selected===parent.index?"white":Theme.colors.text;font.family:Theme.font;verticalAlignment:Text.AlignVCenter;leftPadding:8;font.pixelSize:13}
                }
            }
        }
    }
    Item {
        id:keys;focus:true
        Keys.onPressed:event=>{
            if(event.key===Qt.Key_Escape){root.dismiss();event.accepted=true;}
            else if(event.key===Qt.Key_Down || event.key===Qt.Key_Tab){root.selected=(root.selected+1)%root.choices.length;event.accepted=true;}
            else if(event.key===Qt.Key_Up || event.key===Qt.Key_Backtab){root.selected=(root.selected+root.choices.length-1)%root.choices.length;event.accepted=true;}
            else if(event.key===Qt.Key_Return || event.key===Qt.Key_Enter){root.apply(root.selected);event.accepted=true;}
        }
    }
}

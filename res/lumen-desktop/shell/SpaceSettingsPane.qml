import QtQuick
import Quickshell
import Quickshell.Io
Column {
    id:root
    required property var policy
    spacing:10
    readonly property var state:policy?policy.state:({outputs:[]})
    property var request:null
    property string error:""
    function send(key,value){if(worker.running)return;request={key:key,value:value};error="";worker.running=true;}
    PanelLabel {text:"Display Spaces";font.weight:Font.DemiBold}
    SettingsToggle {objectName:"settings-control-separate-display-spaces";width:parent.width;text:"Separate display spaces";detail:"Keep desktops independent on each display. Turn off to switch linked display contexts together.";checked:root.state.separateDisplays!==false;enabled:!worker.running && !!root.policy;onClicked:root.send("separateDisplays",checked)}
    PanelLabel {text:"Main display"}
    DesktopComboBox {
        objectName:"settings-control-main-display";width:parent.width;textRole:"label";Accessible.name:"Main display"
        readonly property var choices:{const rows=[{label:"Automatic",identity:null}].concat(root.state.outputs || []);if(root.state.primaryDisplay && !rows.some(row=>JSON.stringify(row.identity)===JSON.stringify(root.state.primaryDisplay)))rows.push({label:"Disconnected preferred display",identity:root.state.primaryDisplay});return rows;}
        onChoicesChanged:if(popup.visible)popup.close()
        model:choices
        currentIndex:{const key=JSON.stringify(root.state.primaryDisplay || null);const index=choices.findIndex(row=>JSON.stringify(row.identity)===key);return index<0?-1:index;}
        enabled:!worker.running && !!root.policy
        onActivated:index=>root.send("primaryDisplay",choices[index].identity)
    }
    PanelLabel {width:parent.width;wrapMode:Text.WordWrap;color:Theme.colors.muted;text:"Linked contexts keep existing desktops and windows. The Dock appears on the main display; Window Groups and new paired layouts on other displays require separate spaces."}
    PanelLabel {visible:worker.running;width:parent.width;wrapMode:Text.WordWrap;text:"Applying display preferences…"}
    PanelLabel {visible:!!(root.error || root.state.error);width:parent.width;wrapMode:Text.WordWrap;text:root.error || root.state.error || "";color:Theme.colors.muted}
    Process {
        id:worker;stdinEnabled:true
        command:[Quickshell.env("GLOBAL_MENU_PYTHON") || "python3",Qt.resolvedUrl("display_space_settings.py").toString().replace(/^file:\/\//,"")]
        onStarted:write(JSON.stringify(root.request)+"\n")
        stdout:StdioCollector {onStreamFinished:{try{root.error=JSON.parse(text).error || "";}catch(error){root.error="Display preferences did not respond.";}}}
        onExited:(code,status)=>{if(code!==0)root.error="Display preferences could not be applied.";}
    }
}

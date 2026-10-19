import QtQuick
import Quickshell.Io
Column {
    id:root
    spacing:8
    property var configuration:({})
    property bool nativeAvailable:false
    property bool busy:false
    property string error:""
    property string operation:"status"
    readonly property var corners:["top-left","top-right","bottom-left","bottom-right"]
    readonly property var actions:["disabled","overview","app-windows","show-desktop","notifications-toggle","app-launcher","lock","display-sleep"]
    readonly property var selected:configuration[corners[corner.currentIndex]] || ({action:"disabled",modifiers:[],allowFullscreen:false,allowAdjacent:false})
    function save(){if(!busy){operation="configure";busy=true;worker.running=true;}}
    function load(){if(!busy){operation="status";busy=true;worker.running=true;}}
    function update(key,value){const data=JSON.parse(JSON.stringify(configuration));const name=corners[corner.currentIndex];data[name]=Object.assign({},selected);data[name][key]=value;configuration=data;}
    onVisibleChanged:if(visible)load()
    Component.onCompleted:if(visible)load()
    PanelLabel {text:"Hot Corners";font.weight:Font.DemiBold}
    PanelLabel {width:parent.width;wrapMode:Text.WordWrap;font.pixelSize:11;color:Theme.colors.muted;text:root.nativeAvailable?"Actions run once on corner entry. Leave the corner before activating again. Disabled by default.":"The compositor does not currently provide native corner observation. Saved configuration remains available."}
    DesktopComboBox {id:corner;width:parent.width;model:["Top left","Top right","Bottom left","Bottom right"];Accessible.name:"Configure screen corner"}
    DesktopComboBox {width:parent.width;model:["Disabled","Window Overview","Application windows","Show desktop","Notifications","Applications","Lock screen","Turn displays off"];currentIndex:root.actions.indexOf(root.selected.action);Accessible.name:"Corner action";enabled:!root.busy;onActivated:index=>root.update("action",root.actions[index])}
    PanelLabel {text:"Required modifiers";font.pixelSize:12}
    Flow {width:parent.width;spacing:6
        Repeater {model:["shift","control","alt","super"]
            SettingsToggle {required property string modelData;width:135;text:modelData.charAt(0).toUpperCase()+modelData.slice(1);checked:root.selected.modifiers.includes(modelData);enabled:!root.busy;onClicked:{const modifiers=root.selected.modifiers.filter(value=>value!==modelData);if(checked)modifiers.push(modelData);root.update("modifiers",modifiers);}}
        }
    }
    SettingsToggle {width:parent.width;text:"Allow while an app is full screen";checked:root.selected.allowFullscreen;enabled:!root.busy;onClicked:root.update("allowFullscreen",checked)}
    SettingsToggle {width:parent.width;text:"Allow corners beside another display";detail:"Off avoids accidental activation when moving between displays";checked:root.selected.allowAdjacent;enabled:!root.busy;onClicked:root.update("allowAdjacent",checked)}
    DialogButton {width:160;text:"Save Hot Corners";enabled:!root.busy;onClicked:root.save()}
    PanelLabel {visible:!!root.error;width:parent.width;wrapMode:Text.WordWrap;color:Theme.colors.muted;text:root.error}
    Process {id:worker;command:["lumen-hot-corners",root.operation];stdinEnabled:true
        onStarted:if(root.operation==="configure"){write(JSON.stringify(root.configuration)+"\n");}
        stdout:StdioCollector {onStreamFinished:{try{const data=JSON.parse(text);if(data.corners){root.configuration=data.corners;root.nativeAvailable=data.nativeAvailable===true;}root.error=data.error || "";}catch(e){root.error="Unable to read corner configuration";}}}
        onExited:(code,status)=>{root.busy=false;if(code!==0)root.error="Unable to save or read corner configuration";else if(root.operation==="configure")Qt.callLater(root.load);}
    }
}

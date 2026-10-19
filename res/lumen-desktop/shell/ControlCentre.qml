import QtQuick
import QtQuick.Controls
import Quickshell.Io
import Quickshell.Bluetooth
Column {
    id:root
    required property var shell
    signal panelRequested(string name)
    signal settingsRequested()
    width:300;spacing:10
    property bool editing:false
    property var preferences:Theme.controlCentrePreferences
    property var pending:({})
    property string error:""
    readonly property bool saving:writer.running
    readonly property var names:({wifi:"Wi-Fi",bluetooth:"Bluetooth",sharing:"Sharing",media:"Now Playing",displays:"Displays",appearance:"Appearance",focus:"Focus",brightness:"Brightness",sound:"Sound"})
    function update(key,value){const next=Object.assign({},preferences);next[key]=value;preferences=next;const queued=Object.assign({},pending);queued[key]=value;pending=queued;flush();}
    function flush(){if(writer.running)return;const keys=Object.keys(pending);if(!keys.length)return;const key=keys[0],value=pending[key];const queued=Object.assign({},pending);delete queued[key];pending=queued;writer.command=["lumen-appearance","--set-control-centre",key,JSON.stringify(value)];writer.running=true;}
    function move(id,delta){let order=preferences.order.slice();const at=order.indexOf(id),to=at+delta;if(to<0 || to>=order.length)return;order.splice(at,1);order.splice(to,0,id);update("order",order);}
    function toggle(id){let hidden=preferences.hidden.slice();const at=hidden.indexOf(id);if(at>=0)hidden.splice(at,1);else hidden.push(id);update("hidden",hidden);}
    Connections {target:Theme;function onControlCentrePreferencesChanged(){if(!writer.running && !Object.keys(root.pending).length)root.preferences=Theme.controlCentrePreferences;}}
    Process {id:writer;stdout:StdioCollector {} stderr:StdioCollector {}onExited:(code,status)=>{root.error=code===0?"":"Control layout was not saved. Try again.";Qt.callLater(root.flush);}}
    Flow {visible:!root.editing;width:parent.width;spacing:10
        Repeater {model:root.preferences.order.filter(id=>!root.preferences.hidden.includes(id))
            delegate:Loader {required property string modelData;width:["brightness","sound"].includes(modelData)?root.width:(root.width-10)/2;height:modelData==="media"?130:62
                sourceComponent:({wifi:componentWifi,bluetooth:componentBluetooth,sharing:componentSharing,media:componentMedia,displays:componentDisplays,appearance:componentAppearance,focus:componentFocus,brightness:componentBrightness,sound:componentSound})[modelData]
            }
        }
    }
    ControlRow {visible:!root.editing;width:parent.width;text:"VPN & Private Network";detail:root.shell.vpn.mullvad.state==="connected"?"Mullvad connected · "+(root.shell.vpn.tailscale.online?"Tailscale connected":"Tailscale off"):root.shell.vpn.tailscale.online?"Tailscale connected · Mullvad off":"Both disconnected";onClicked:root.panelRequested("VPN")}
    Column {visible:root.editing;width:parent.width;spacing:5
        PanelLabel {text:"Edit controls";font.weight:Font.DemiBold}
        Repeater {model:root.preferences.order
            delegate:Row {required property string modelData;required property int index;width:root.width;spacing:4
                ControlRow {width:parent.width-84;height:32;text:(root.preferences.hidden.includes(modelData)?"    ":"✓  ")+root.names[modelData];Accessible.name:(root.preferences.hidden.includes(modelData)?"Show ":"Hide ")+root.names[modelData];onClicked:root.toggle(modelData)}
                DialogButton {width:38;height:30;text:"↑";Accessible.name:"Move "+root.names[parent.modelData]+" earlier";enabled:parent.index>0;onClicked:root.move(parent.modelData,-1)}
                DialogButton {width:38;height:30;text:"↓";Accessible.name:"Move "+root.names[parent.modelData]+" later";enabled:parent.index<root.preferences.order.length-1;onClicked:root.move(parent.modelData,1)}
            }
        }
        PanelLabel {width:parent.width;wrapMode:Text.WordWrap;text:"Show, hide and reorder supported controls. Hardware availability still determines whether a control can be used.";color:Theme.colors.muted}
    }
    Row {width:parent.width;spacing:8
        DialogButton {width:92;height:28;text:root.editing?"Done":"Edit controls";onClicked:root.editing=!root.editing}
        DialogButton {width:parent.width-100;height:28;text:"System Settings…";onClicked:root.settingsRequested()}
    }
    PanelLabel {visible:root.error!=="";width:parent.width;wrapMode:Text.WordWrap;text:root.error;color:Theme.colors.muted}
    Component {id:componentWifi;ControlTile {label:"Wi-Fi";symbol:"wifi";unavailable:!root.shell.systemState.wifi.available;active:root.shell.systemState.wifi.state==="COMPLETED";subtitle:active?"Connected":"Disconnected";onClicked:root.panelRequested("Wi-Fi")}}
    Component {id:componentBluetooth;ControlTile {label:"Bluetooth";symbol:"bluetooth";unavailable:!Bluetooth.defaultAdapter;active:!!Bluetooth.defaultAdapter && Bluetooth.defaultAdapter.enabled;subtitle:active?"On":"Off";onClicked:root.panelRequested("Bluetooth")}}
    Component {id:componentSharing;ControlTile {label:"Sharing";subtitle:"LocalSend";symbol:"share";unavailable:false;accessibleLabel:"Open LocalSend nearby sharing";onClicked:{root.shell.claimPopup("");AppLaunch.run(["localsend_app"]);}}}
    Component {id:componentMedia;MediaControl {onExpandRequested:root.panelRequested("Now Playing")}}
    Component {id:componentDisplays;ControlTile {label:"Displays";subtitle:"Settings";symbol:"screen";unavailable:false;onClicked:root.shell.openSettings("Displays")}}
    Component {id:componentAppearance;ControlTile {label:"Appearance";subtitle:"Settings";symbol:"contrast";unavailable:false;onClicked:root.shell.openSettings("Appearance")}}
    Component {id:componentFocus;ControlTile {label:"Focus";toggle:true;symbol:"moon";unavailable:!root.shell.notifications.available;active:root.shell.notifications.dnd;subtitle:active?root.shell.notifications.focusState.name:"Off";onClicked:root.panelRequested("Focus")}}
    Component {id:componentBrightness;PanelSurface {radius:24;BrightnessControl {x:16;y:12;width:parent.width-32}}}
    Component {id:componentSound;PanelSurface {radius:24;SoundControl {x:16;y:12;width:parent.width-32;shell:root.shell;compact:true}}}
}

import QtQuick
import QtQuick.Controls
import Quickshell.Io
import Quickshell.Bluetooth
import "BatteryModel.js" as BatteryModel
import "NetworkState.js" as NetworkState
Column {
    id:root
    required property var shell
    signal panelRequested(string name)
    signal settingsRequested()
    width:300;spacing:10
    property bool editing:false
    readonly property bool singleColumn:Theme.textSize(12)>12 || width<260
    readonly property var power:root.shell.systemState && root.shell.systemState.power?root.shell.systemState.power:null
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
    Row {visible:!root.editing && !!root.power && root.power.available && root.power.present;width:parent.width;spacing:8
        BatteryGlyph {anchors.verticalCenter:parent.verticalCenter;percentage:root.power?root.power.percentage:0;charging:!!root.power && root.power.state==="charging"}
        Column {width:parent.width-32
            PanelLabel {width:parent.width;text:root.power?BatteryModel.description(root.power):"";elide:Text.ElideRight}
            PanelLabel {width:parent.width;visible:text!=="";text:root.power?BatteryModel.estimate(root.power):"";color:Theme.colors.muted;font.pixelSize:Theme.textSize(11);elide:Text.ElideRight}
        }
    }
    Flow {visible:!root.editing;width:parent.width;spacing:10
        Repeater {model:root.preferences.order.filter(id=>!root.preferences.hidden.includes(id))
            delegate:Loader {required property string modelData;width:root.singleColumn || ["brightness","sound"].includes(modelData)?root.width:(root.width-10)/2;height:Math.max(modelData==="media"?130:62,item?item.implicitHeight:0)
                sourceComponent:({wifi:componentWifi,bluetooth:componentBluetooth,sharing:componentSharing,media:componentMedia,displays:componentDisplays,appearance:componentAppearance,focus:componentFocus,brightness:componentBrightness,sound:componentSound})[modelData]
            }
        }
    }
    ControlRow {visible:!root.editing;width:parent.width;text:"VPN";detail:root.shell.vpn.mullvad.state==="connected"?"Mullvad connected"+(root.shell.vpn.tailscale.online?" · Tailscale connected":""):root.shell.vpn.tailscale.online?"Tailscale connected":"Disconnected";onClicked:root.panelRequested("VPN")}
    ControlRow {visible:!root.editing;width:parent.width;text:"Capture observations…";detail:"PipeWire-visible activity";onClicked:root.panelRequested("Capture")}
    Column {visible:root.editing;width:parent.width;spacing:5
        PanelLabel {text:"Edit controls";font.weight:Font.DemiBold}
        Repeater {model:root.preferences.order
            delegate:Row {required property string modelData;required property int index;width:root.width;spacing:4
                ControlRow {width:parent.width-84;height:Math.max(32,implicitHeight);text:(root.preferences.hidden.includes(modelData)?"    ":"✓  ")+root.names[modelData];Accessible.name:(root.preferences.hidden.includes(modelData)?"Show ":"Hide ")+root.names[modelData];onClicked:root.toggle(modelData)}
                DialogButton {width:38;height:Math.max(30,implicitHeight);text:"↑";Accessible.name:"Move "+root.names[parent.modelData]+" earlier";enabled:parent.index>0;onClicked:root.move(parent.modelData,-1)}
                DialogButton {width:38;height:Math.max(30,implicitHeight);text:"↓";Accessible.name:"Move "+root.names[parent.modelData]+" later";enabled:parent.index<root.preferences.order.length-1;onClicked:root.move(parent.modelData,1)}
            }
        }
        PanelLabel {width:parent.width;wrapMode:Text.WordWrap;text:"Show, hide and reorder supported controls. Hardware availability still determines whether a control can be used.";color:Theme.colors.muted}
    }
    Row {width:parent.width;spacing:8
        DialogButton {width:Math.min(parent.width/2-4,implicitWidth);text:root.editing?"Done":"Edit controls";onClicked:root.editing=!root.editing}
        DialogButton {width:parent.width-parent.children[0].width-8;text:"System Settings…";onClicked:root.settingsRequested()}
    }
    PanelLabel {visible:root.error!=="";width:parent.width;wrapMode:Text.WordWrap;text:root.error;color:Theme.colors.muted}
    Component {id:componentWifi;ControlTile {label:"Wi-Fi";symbol:"wifi";unavailable:NetworkState.state(root.shell.systemState.wifi).unavailable;active:NetworkState.state(root.shell.systemState.wifi).active;subtitle:NetworkState.state(root.shell.systemState.wifi).label;onClicked:root.panelRequested("Wi-Fi")}}
    Component {id:componentBluetooth;ControlTile {label:"Bluetooth";symbol:"bluetooth";unavailable:!Bluetooth.defaultAdapter;active:!!Bluetooth.defaultAdapter && Bluetooth.defaultAdapter.enabled;subtitle:active?"On":"Off";onClicked:root.panelRequested("Bluetooth")}}
    Component {id:componentSharing;ControlTile {label:"Sharing";subtitle:"LocalSend";symbol:"share";unavailable:false;accessibleLabel:"Open LocalSend nearby sharing";onClicked:{root.shell.claimPopup("");AppLaunch.run(["localsend_app"]);}}}
    Component {id:componentMedia;MediaControl {onExpandRequested:root.panelRequested("Now Playing")}}
    Component {id:componentDisplays;ControlTile {label:"Displays";subtitle:"Settings";symbol:"screen";unavailable:false;onClicked:root.shell.openSettings("Displays")}}
    Component {id:componentAppearance;ControlTile {label:"Appearance";subtitle:"Settings";symbol:"contrast";unavailable:false;onClicked:root.shell.openSettings("Appearance")}}
    Component {id:componentFocus;ControlTile {label:"Focus";toggle:true;symbol:"moon";unavailable:!root.shell.notifications.available;active:root.shell.notifications.dnd;subtitle:active?root.shell.notifications.focusState.name:"Off";onClicked:root.panelRequested("Focus")}}
    Component {id:componentBrightness;PanelSurface {radius:24;implicitHeight:brightnessContent.implicitHeight+24;BrightnessControl {id:brightnessContent;x:16;y:12;width:parent.width-32}}}
    Component {id:componentSound;PanelSurface {radius:24;implicitHeight:soundContent.implicitHeight+24;SoundControl {id:soundContent;x:16;y:12;width:parent.width-32;shell:root.shell;compact:true}}}
}

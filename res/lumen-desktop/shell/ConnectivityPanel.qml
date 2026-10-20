import QtQuick
import Quickshell
import Quickshell.Bluetooth
import Quickshell.Io
import QtQuick.Controls
import "NetworkState.js" as NetworkState
PanelSurface {
    id: root
    required property string kind
    required property var controls
    property bool embedded:false
    property string pairingName:""
    property string pairingPath:""
    property string promptKind:""
    property string passcode:""
    property string pairingMessage:""
    property bool discoveryOwned:false
    function stopDiscovery(){if(discoveryOwned && adapter){adapter.discovering=false;discoveryOwned=false;}}
    onVisibleChanged:if(!visible)stopDiscovery()
    Component.onDestruction:stopDiscovery()

    signal backRequested()
    width:300;height:content.height+28
    radius:embedded?12:24
    layer.enabled:!embedded && GraphicsInfo.api !== GraphicsInfo.Software
    color:embedded?Theme.colors.surface:Theme.colors.background
    readonly property var adapter: Bluetooth.defaultAdapter
    readonly property var radio: adapter ? (controls.bluetoothRadios[adapter.dbusPath] || ({})) : ({})
    Column {
        id:content;x:14;y:14;width:parent.width-28;spacing:6
        ControlRow {visible:!root.embedded;width:parent.width;text:"‹  Control Centre";onClicked:root.backRequested()}
        PanelLabel {text:root.kind;font.pixelSize:14;font.weight:Font.DemiBold}
        Column {
            visible:root.kind==="Wi-Fi";width:parent.width;spacing:4
            PanelLabel {width:parent.width;wrapMode:Text.WordWrap;text:NetworkState.state(root.controls.wifi).detail;color:Theme.colors.muted}
            Repeater {
                model:root.controls.wifi.networks
                ControlRow {required property var modelData;width:content.width;text:modelData.name;selected:modelData.connected;enabled:!root.controls.busy && !modelData.connected;onClicked:root.controls.act("wifi-connect",modelData.id)}
            }
            ControlRow {width:parent.width;visible:root.controls.wifi.available;text:root.controls.wifi.state==="DISCONNECTED"?"Reconnect":"Disconnect";enabled:!root.controls.busy;onClicked:root.controls.act(root.controls.wifi.state==="DISCONNECTED"?"wifi-reconnect":"wifi-disconnect")}
            PanelLabel {width:parent.width;wrapMode:Text.WordWrap;text:root.controls.wifi.available?"Choose a saved network to connect.":"Wi-Fi controls are unavailable. Check network settings or use Ethernet.";font.pixelSize:11;color:Theme.colors.muted}
        }
        Column {
            visible:root.kind==="Bluetooth";width:parent.width;spacing:4
            ControlRow {width:parent.width;text:!root.adapter?"Bluetooth unavailable":root.adapter.enabled?"Turn Bluetooth Off":"Turn Bluetooth On";enabled:!root.controls.busy && !!root.adapter && !root.radio.hard;onClicked:root.controls.act(root.adapter.enabled?"bluetooth-off":"bluetooth-on",root.adapter.dbusPath)}
            PanelLabel {visible:!!root.adapter && (!!root.radio.hard || !!root.radio.soft);text:root.radio.hard?"Bluetooth is blocked by a hardware switch":"Bluetooth is turned off in software. Turn it on to find devices.";width:parent.width;wrapMode:Text.WordWrap;color:Theme.colors.muted}
            Repeater {
                model:root.adapter ? root.adapter.devices.values.filter(d=>d.paired) : []
                ControlRow {
                    required property var modelData
                    width:content.width;text:modelData.name
                    detail:modelData.state===BluetoothDeviceState.Connecting?"Connecting…":modelData.state===BluetoothDeviceState.Disconnecting?"Disconnecting…":modelData.connected?"Connected"+(modelData.batteryAvailable?" · "+Math.round(modelData.battery*100)+"%":""):"Not Connected"
                    enabled:!root.controls.busy && !!root.adapter && root.adapter.enabled && (modelData.state===BluetoothDeviceState.Connected || modelData.state===BluetoothDeviceState.Disconnected)
                    onClicked:root.controls.act(modelData.connected?"bluetooth-disconnect":"bluetooth-connect",modelData.dbusPath)
                }
            }
            ControlRow {width:parent.width;text:root.adapter && root.adapter.discovering?"Stop Finding Devices":"Find Nearby Devices";enabled:!!root.adapter && root.adapter.enabled && !root.controls.busy;onClicked:{const stop=root.adapter.discovering;root.discoveryOwned=!stop;root.adapter.discovering=!stop;}}
            Repeater {model:root.adapter && root.adapter.discovering?root.adapter.devices.values.filter(d=>!d.paired):[]
                ControlRow {required property var modelData;width:content.width;text:modelData.name;detail:"Pair…";enabled:!pairing.running;onClicked:{root.pairingName=modelData.name;root.pairingPath=modelData.dbusPath;root.promptKind="";root.pairingMessage="Connecting…";root.passcode="";pairDialog.open();pairing.running=true;}}
            }
            ControlRow {width:parent.width;text:"Bluetooth Settings…";detail:"Additional Bluetooth options";onClicked:AppLaunch.run(["blueman-manager"])}
        }
        PanelLabel {visible:text!=="";width:parent.width;wrapMode:Text.WordWrap;text:root.controls.error;color:Theme.colors.muted}
    }
    Timer {interval:30000;running:root.discoveryOwned;onTriggered:root.stopDiscovery()}
    Popup {id:pairDialog;modal:true;focus:true;width:Math.min(340,root.width-12);x:(root.width-width)/2;y:20;padding:16;closePolicy:Popup.NoAutoClose
        background:Rectangle {radius:12;color:Theme.colors.background;border.color:Theme.outline}
        contentItem:Column {spacing:12;width:pairDialog.width-32
            PanelLabel {width:parent.width;text:"Pair with "+root.pairingName;textFormat:Text.PlainText;wrapMode:Text.WordWrap;font.weight:Font.DemiBold}
            PanelLabel {width:parent.width;text:root.promptKind==="confirm"?"Confirm this code matches the device:":root.promptKind==="display"?"Enter this code on the device:":root.promptKind==="authorize"?"Allow this device to pair?":root.promptKind==="pin" || root.promptKind==="passkey"?"Enter the code provided by the device:":root.pairingMessage;wrapMode:Text.WordWrap}
            PanelLabel {visible:!!root.passcode;text:root.passcode;font.pixelSize:25;font.weight:Font.DemiBold}
            TextField {id:pin;visible:["pin","passkey"].includes(root.promptKind);width:parent.width;color:Theme.colors.text;font.family:Theme.font;Accessible.name:"Pairing code";background:Rectangle {radius:6;color:Theme.colors.field;border.color:Theme.colors.accent}}
            Row {spacing:8
                DialogButton {width:110;text:pairing.running?"Cancel":"Close";onClicked:{if(pairing.running)pairing.write(JSON.stringify({action:"cancel"})+"\n");pairDialog.close();}}
                DialogButton {width:130;text:"Pair";primary:true;visible:["confirm","authorize","pin","passkey"].includes(root.promptKind);onClicked:{pairing.write(JSON.stringify({accept:true,value:pin.text})+"\n");root.promptKind="";root.pairingMessage="Pairing…";}}
            }
        }
    }
    Process {id:pairing;stdinEnabled:true
        onExited:root.promptKind=""
        command:[Quickshell.env("GLOBAL_MENU_PYTHON") || "python3",Qt.resolvedUrl("bluetooth_pair.py").toString().replace("file://",""),root.pairingPath]
        stdout:SplitParser {onRead:line=>{try{const d=JSON.parse(line);if(d.error)root.pairingMessage=d.error;else if(d.event==="prompt"){root.promptKind=d.kind;root.passcode=d.code || "";pin.text="";}else if(d.event==="paired"){root.promptKind="";root.pairingMessage="Paired successfully. Select the device to connect.";}else if(d.event==="cancelled"){root.promptKind="";root.pairingMessage="Pairing cancelled";}}catch(e){root.pairingMessage="Unable to read pairing response";}}}
    }

}

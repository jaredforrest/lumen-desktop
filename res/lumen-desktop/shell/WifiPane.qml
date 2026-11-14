import QtQuick
import QtQuick.Controls
import "NetworkState.js" as NetworkState
Column {
    id:root
    required property var controls
    spacing:10
    readonly property var wifi:controls.wifi || ({})
    readonly property var state:NetworkState.state(wifi)
    readonly property bool managed:wifi.backend==="network-manager"
    readonly property var networks:wifi.networks || []
    property var joining:({})
    function connect(network) {
        if(network.connected || controls.busy)return;
        if(!managed || !network.secured){controls.act("wifi-connect",network.id);return;}
        joining=network;password.text="";controls.error="";joinDialog.open();
    }
    SettingsToggle {
        objectName:"wifi-power";width:parent.width;text:"Wi-Fi"
        detail:root.state.phase==="connected"?"On":root.state.label
        checked:root.wifi.enabled===true || (!root.managed && root.wifi.available)
        enabled:root.managed && !root.controls.busy
        onClicked:root.controls.act(checked?"wifi-on":"wifi-off")
    }
    Rectangle {
        visible:root.state.active;width:parent.width;height:connectedContent.implicitHeight+20;radius:10
        color:Theme.colors.surface;border.color:Theme.hairline
        Column {id:connectedContent;x:10;y:10;width:parent.width-20;spacing:5
            PanelLabel {width:parent.width;text:root.wifi.name || "Connected network";font.weight:Font.DemiBold}
            PanelLabel {width:parent.width;text:root.state.label;font.pixelSize:11;color:Theme.colors.muted;wrapMode:Text.WordWrap}
            ControlRow {visible:root.wifi.connectivity==="portal";width:parent.width;text:"Open Sign-In Page…";onClicked:AppLaunch.run(["xdg-open","http://neverssl.com/"])}
            ControlRow {width:parent.width;text:"Disconnect";enabled:!root.controls.busy;onClicked:root.controls.act("wifi-disconnect")}
        }
    }
    Item {
        width:parent.width;height:32;visible:root.wifi.available && root.state.phase!=="disabled"
        PanelLabel {anchors.left:parent.left;anchors.leftMargin:8;anchors.verticalCenter:parent.verticalCenter;text:"Other Networks";font.weight:Font.DemiBold;font.pixelSize:11;color:Theme.colors.muted}
        AbstractButton {
            objectName:"wifi-refresh";anchors.right:parent.right;anchors.rightMargin:4;anchors.verticalCenter:parent.verticalCenter
            width:88;height:30;padding:6
            text:root.controls.busy && root.controls.pendingAction==="wifi-scan"?"Scanning…":"Refresh"
            enabled:root.managed && !root.controls.busy
            Accessible.name:"Refresh nearby Wi-Fi networks"
            contentItem:PanelLabel {text:parent.text;horizontalAlignment:Text.AlignHCenter;verticalAlignment:Text.AlignVCenter;font.pixelSize:11;opacity:parent.enabled?1:0.5}
            background:Rectangle {radius:7;color:parent.hovered || parent.activeFocus?Theme.colors.hover:"transparent";border.width:parent.activeFocus?1:0;border.color:Theme.colors.accent}
            onClicked:root.controls.act("wifi-scan")
        }
    }
    Flickable {
        visible:root.wifi.available && root.state.phase!=="disabled"
        width:parent.width;height:Math.min(networkList.implicitHeight,250);contentHeight:networkList.implicitHeight;clip:true
        boundsBehavior:Flickable.StopAtBounds
        ScrollBar.vertical:ScrollBar {policy:ScrollBar.AsNeeded}
        Column {id:networkList;width:parent.width;spacing:3
            Repeater {
                model:root.networks.filter(n=>!n.connected)
                ControlRow {
                    required property var modelData
                    required property int index
                    objectName:"wifi-network-"+index
                    width:networkList.width;text:modelData.name
                    detail:root.managed?(modelData.enterprise?"Enterprise · ":modelData.secured?"Secured · ":"Open · ")+(modelData.signal>=70?"Strong signal":modelData.signal>=40?"Good signal":"Weak signal"):"Saved network"
                    enabled:!root.controls.busy
                    onClicked:root.connect(modelData)
                }
            }
            PanelLabel {visible:root.networks.filter(n=>!n.connected).length===0;width:parent.width;padding:8;wrapMode:Text.WordWrap;text:"No other networks found. Refresh to search again.";color:Theme.colors.muted;font.pixelSize:11}
        }
    }
    PanelLabel {visible:root.controls.busy;width:parent.width;text:root.controls.pendingAction==="wifi-connect"?"Connecting…":root.controls.pendingAction==="wifi-scan"?"Looking for nearby networks…":"Updating Wi-Fi…";color:Theme.colors.muted;font.pixelSize:11}
    ControlRow {width:parent.width;text:"Advanced Wi-Fi Settings…";detail:"Saved networks and enterprise sign-in";onClicked:AppLaunch.run(["foot","--app-id=lumen-network-settings","--title=Wi-Fi Settings","nmtui","connect"])}
    Popup {
        id:joinDialog;modal:true;focus:true;width:root.width;x:0;y:40;padding:14
        closePolicy:Popup.CloseOnEscape
        onClosed:password.text=""
        background:Rectangle {radius:12;color:Theme.colors.background;border.color:Theme.outline}
        contentItem:Column {spacing:12;width:joinDialog.width-28
            PanelLabel {width:parent.width;text:"Join “"+(root.joining.name || "")+"”";wrapMode:Text.WordWrap;font.weight:Font.DemiBold}
            PanelLabel {width:parent.width;text:root.joining.enterprise?"Configure this network in Advanced Wi-Fi Settings.":"Enter the network password, or leave blank to use its saved password.";wrapMode:Text.WordWrap;color:Theme.colors.muted;font.pixelSize:11}
            DesktopTextField {id:password;objectName:"wifi-password";width:parent.width;visible:!root.joining.enterprise;echoMode:TextInput.Password;placeholderText:"Password";Accessible.name:"Wi-Fi password";onAccepted:join.clicked()}
            Row {spacing:8
                DialogButton {width:96;text:"Cancel";onClicked:joinDialog.close()}
                DialogButton {id:join;objectName:"wifi-join";width:110;text:"Join";primary:true;visible:!root.joining.enterprise;onClicked:{const secret=password.text;password.text="";joinDialog.close();root.controls.act("wifi-connect",root.joining.id,secret);}}
            }
        }
        onOpened:password.forceActiveFocus()
    }
}

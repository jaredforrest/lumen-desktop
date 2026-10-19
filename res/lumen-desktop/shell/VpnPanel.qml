import QtQuick
import QtQuick.Controls

Column {
    id: root
    required property var service
    property bool compact:false
    signal backRequested()
    signal settingsRequested()
    width:compact?300:parent.width
    spacing:12
    ControlRow {visible:root.compact;width:parent.width;text:"‹  Control Centre";onClicked:root.backRequested()}
    PanelLabel {text:root.compact?"VPN":"VPN & Private Network";font.pixelSize:root.compact?15:19;font.weight:Font.DemiBold}
    PanelLabel {width:parent.width;wrapMode:Text.WordWrap;text:"Mullvad protects internet traffic. Tailscale connects your private devices. Running both together may need additional routing configuration.";color:Theme.colors.muted;font.pixelSize:11}
    PanelSurface {
        width:parent.width;height:mullvadBody.height+28
        Column {
            id:mullvadBody;x:14;y:14;width:parent.width-28;spacing:8
            Row {width:parent.width;spacing:8
                Rectangle {width:10;height:10;radius:5;anchors.verticalCenter:parent.verticalCenter;color:root.service.mullvad.state==="connected"?"#30b65c":Theme.colors.muted}
                PanelLabel {text:"Mullvad VPN";font.weight:Font.DemiBold}
            }
            PanelLabel {width:parent.width;wrapMode:Text.WordWrap;text:!root.service.mullvad.installed?"Unavailable":!root.service.mullvad.account?"Account sign-in required":root.service.mullvad.state==="connected"?"Connected"+(root.service.mullvad.location?" · "+root.service.mullvad.location:""):root.service.mullvad.state==="connecting"?"Connecting…":root.service.mullvad.state==="disconnecting"?"Disconnecting…":"Disconnected";color:Theme.colors.muted}
            PanelLabel {visible:!!root.service.mullvad.relay;width:parent.width;text:"Relay: "+root.service.mullvad.relay;color:Theme.colors.muted;font.pixelSize:11}
            Row {width:parent.width;spacing:8
                DialogButton {width:parent.width;visible:!root.service.mullvad.account;text:"Sign in…";enabled:root.service.mullvad.installed && !root.service.busy;onClicked:loginDialog.open()}
                DialogButton {width:parent.width;visible:root.service.mullvad.account;text:root.service.mullvad.state==="connected"?"Disconnect":"Connect";primary:root.service.mullvad.state!=="connected";enabled:root.service.mullvad.installed && !root.service.busy;onClicked:root.service.run(root.service.mullvad.state==="connected"?"mullvad-disconnect":"mullvad-connect")}
            }
            Row {visible:root.service.mullvad.account;width:parent.width;spacing:6
                PanelLabel {height:28;verticalAlignment:Text.AlignVCenter;text:"Location";width:60}
                DesktopComboBox {id:country;width:parent.width-66;model:["Automatic","United Kingdom","United States","Sweden","Germany","Netherlands","Switzerland"];currentIndex:Math.max(0,["any","gb","us","se","de","nl","ch"].indexOf((root.service.mullvad.relay || "").split(" ").slice(-1)[0]));Accessible.name:"Mullvad country";enabled:!root.service.busy;onActivated:index=>root.service.run("mullvad-location",{country:["any","gb","us","se","de","nl","ch"][index]})}
            }
        }
    }
    PanelSurface {
        width:parent.width;height:tailscaleBody.height+28
        Column {
            id:tailscaleBody;x:14;y:14;width:parent.width-28;spacing:8
            Row {width:parent.width;spacing:8
                Rectangle {width:10;height:10;radius:5;anchors.verticalCenter:parent.verticalCenter;color:root.service.tailscale.online?"#30b65c":Theme.colors.muted}
                PanelLabel {text:"Tailscale";font.weight:Font.DemiBold}
            }
            PanelLabel {width:parent.width;wrapMode:Text.WordWrap;text:!root.service.tailscale.installed?"Unavailable":root.service.tailscale.online?"Connected"+(root.service.tailscale.tailnet?" · "+root.service.tailscale.tailnet:"")+" · "+root.service.tailscale.peers+" peers online":root.service.tailscale.state==="NeedsLogin"?"Sign-in required":root.service.tailscale.state==="Stopped"?"Stopped":root.service.tailscale.state;color:Theme.colors.muted}
            DialogButton {width:parent.width;text:root.service.tailscale.online?"Disconnect":"Connect";enabled:root.service.tailscale.installed && !root.service.busy;onClicked:root.service.run(root.service.tailscale.online?"tailscale-disconnect":"tailscale-connect")}
            PanelLabel {width:parent.width;text:"Internet exit node";font.pixelSize:11;color:Theme.colors.muted}
            DesktopComboBox {
                id:exitChoice;width:parent.width
                model:[{id:"",name:"None · use normal internet"}].concat(root.service.tailscale.exitNodes || [])
                textRole:"name";currentIndex:Math.max(0,model.findIndex(n=>n.id===root.service.tailscale.exitNode))
                Accessible.name:"Tailscale exit node";enabled:root.service.tailscale.online && !root.service.busy
                onActivated:index=>root.service.run("tailscale-exit",{id:model[index].id})
            }
            PanelLabel {visible:root.service.tailscale.exitNodes.length===0;width:parent.width;wrapMode:Text.WordWrap;text:"No exit nodes available. Mullvad exit nodes require the separate Tailscale add-on.";font.pixelSize:11;color:Theme.colors.muted}
        }
    }
    StatusMessage {width:parent.width;text:root.service.error || root.service.mullvad.error || root.service.tailscale.error || "";severity:"error"}
    PanelLabel {visible:root.service.mullvad.state==="connected" && root.service.tailscale.online;width:parent.width;wrapMode:Text.WordWrap;text:"Both clients are connected. Their routing and DNS compatibility has not been verified on this network.";font.pixelSize:11;color:Theme.colors.muted}
    ControlRow {visible:root.compact;width:parent.width;text:"VPN Settings…";onClicked:root.settingsRequested()}
    DialogButton {visible:!root.compact;width:parent.width;text:"Refresh Status";enabled:!root.service.busy;onClicked:root.service.refresh()}
    Dialog {
        id:loginDialog;modal:true;title:"Sign in to Mullvad";standardButtons:Dialog.Ok|Dialog.Cancel
        anchors.centerIn:Overlay.overlay;width:Math.min(380,Overlay.overlay?Overlay.overlay.width-32:380)
        onAccepted:{root.service.run("mullvad-login",{account:accountNumber.text.trim()});accountNumber.text="";}
        onRejected:accountNumber.text=""
        contentItem:Column {spacing:10
            PanelLabel {width:parent.width;wrapMode:Text.WordWrap;text:"Enter your 16-digit Mullvad account number. It is sent to the local Mullvad client and is not saved by Lumen."}
            TextField {id:accountNumber;width:parent.width;echoMode:TextInput.Password;inputMethodHints:Qt.ImhDigitsOnly;maximumLength:16;Accessible.name:"Mullvad account number"}
        }
    }
}

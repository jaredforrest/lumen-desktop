import QtQuick
import QtQuick.Controls

Column {
    id: root
    required property var service
    property bool compact: false
    signal backRequested()
    signal settingsRequested()
    width: compact ? 300 : parent.width
    spacing: compact ? 10 : 13

    function expiryLabel(value) {
        if (!value || !/^\d{4}-\d{2}-\d{2}$/.test(value)) return "Signed in"
        const parts = value.split("-")
        return "Expires " + Qt.formatDate(new Date(Number(parts[0]), Number(parts[1]) - 1, Number(parts[2])), "d MMM yyyy")
    }

    ControlRow { visible: root.compact; width: parent.width; text: "‹  Control Centre"; onClicked: root.backRequested() }
    PanelLabel { width: parent.width; text: "VPN"; font.pixelSize: root.compact ? 15 : 22; font.weight: Font.DemiBold }
    PanelLabel { visible: !root.compact; text: "VPN services"; color: Theme.colors.muted; font.pixelSize: 11 }

    Rectangle {
        width: parent.width; height: services.height
        radius: 10; color: Theme.colors.surface; border.color: Theme.hairline; clip: true
        Column {
            id: services; width: parent.width
            Repeater {
                model: 2
                Item {
                    required property int index
                    readonly property bool isMullvad: index === 0
                    readonly property bool connected: isMullvad ? root.service.mullvad.state === "connected" : root.service.tailscale.online
                    readonly property string label: isMullvad ? "Mullvad VPN" : "Tailscale"
                    width: services.width; height: 60
                    AbstractButton {
                        id: toggle
                        anchors.fill: parent
                        enabled: !root.service.busy && (parent.isMullvad
                            ? root.service.mullvad.installed && root.service.mullvad.account
                            : root.service.tailscale.installed)
                        Accessible.name: parent.label + ", " + stateLabel.text + ", " + (parent.connected ? "disconnect" : "connect")
                        onClicked: root.service.run(parent.isMullvad
                            ? (parent.connected ? "mullvad-disconnect" : "mullvad-connect")
                            : (parent.connected ? "tailscale-disconnect" : "tailscale-connect"))
                        background: Rectangle { color: toggle.hovered || toggle.activeFocus ? Theme.colors.hover : "transparent" }
                        contentItem: Item {
                            Rectangle {
                                x: 14; anchors.verticalCenter: parent.verticalCenter
                                width: 28; height: 28; radius: 7
                                color: toggle.parent.isMullvad ? "#d74540" : "#77767d"
                                SystemGlyph { anchors.centerIn: parent; name: "network"; width: 18; height: 18; ink: "white" }
                            }
                            Column {
                                x: 52; width: parent.width - x - 66
                                anchors.verticalCenter: parent.verticalCenter; spacing: 2
                                PanelLabel { width: parent.width; text: toggle.parent.label; font.weight: Font.Medium; elide: Text.ElideRight }
                                PanelLabel {
                                    id: stateLabel; width: parent.width; elide: Text.ElideRight
                                    color: Theme.colors.muted; font.pixelSize: 11
                                    text: toggle.parent.isMullvad
                                        ? (!root.service.mullvad.installed ? "Not installed"
                                           : !root.service.mullvad.account ? "Sign-in required"
                                           : toggle.parent.connected ? "Connected" + (root.service.mullvad.location ? " · " + root.service.mullvad.location : "")
                                           : root.service.mullvad.state === "connecting" ? "Connecting…"
                                           : root.service.mullvad.state === "disconnecting" ? "Disconnecting…" : "Disconnected")
                                        : (!root.service.tailscale.installed ? "Not installed"
                                           : toggle.parent.connected ? "Connected" + (root.service.tailscale.tailnet ? " · " + root.service.tailscale.tailnet : "")
                                           : root.service.tailscale.state === "NeedsLogin" ? "Sign-in required"
                                           : root.service.tailscale.state === "Stopped" ? "Disconnected" : root.service.tailscale.state)
                                }
                            }
                            Rectangle {
                                anchors.right: parent.right; anchors.rightMargin: 14; anchors.verticalCenter: parent.verticalCenter
                                width: 34; height: 21; radius: 11
                                color: toggle.parent.connected ? Theme.colors.accent : Theme.colors.field
                                opacity: toggle.enabled ? 1 : 0.55
                                Rectangle {
                                    x: toggle.parent.connected ? 15 : 2; y: 2
                                    width: 17; height: 17; radius: 9; color: "white"
                                    border.color: Theme.dark ? "#33000000" : "#22000000"
                                }
                            }
                        }
                    }
                    Rectangle {
                        visible: index === 0; x: 52; y: 59
                        width: parent.width - x; height: 1; color: Theme.hairline
                    }
                }
            }
        }
    }

    PanelLabel { visible: !root.compact; text: "Mullvad VPN"; color: Theme.colors.muted; font.pixelSize: 11 }
    Rectangle {
        visible: !root.compact; width: parent.width; height: mullvadDetails.height
        radius: 10; color: Theme.colors.surface; border.color: Theme.hairline; clip: true
        Column {
            id: mullvadDetails; width: parent.width
            Item {
                width: parent.width; height: 44
                PanelLabel { x: 14; anchors.verticalCenter: parent.verticalCenter; text: "Account" }
                PanelLabel {
                    anchors.right: signInButton.visible ? signInButton.left : parent.right
                    anchors.rightMargin: 14; anchors.verticalCenter: parent.verticalCenter
                    text: root.service.mullvad.account ? root.expiryLabel(root.service.mullvad.expires) : "Not signed in"
                    color: Theme.colors.muted; font.pixelSize: 11
                }
                DialogButton {
                    id: signInButton; visible: !root.service.mullvad.account && root.service.mullvad.installed
                    anchors.right: parent.right; anchors.rightMargin: 12; anchors.verticalCenter: parent.verticalCenter
                    text: "Sign In…"; enabled: !root.service.busy; onClicked: loginDialog.open()
                }
            }
            Rectangle { x: 14; width: parent.width - x; height: 1; color: Theme.hairline }
            Item {
                width: parent.width; height: 44
                PanelLabel { x: 14; anchors.verticalCenter: parent.verticalCenter; text: "Location" }
                DesktopComboBox {
                    anchors.right: parent.right; anchors.rightMargin: 12; anchors.verticalCenter: parent.verticalCenter
                    width: Math.min(225, parent.width - 105)
                    model: ["Automatic", "United Kingdom", "United States", "Sweden", "Germany", "Netherlands", "Switzerland"]
                    currentIndex: Math.max(0, ["any", "gb", "us", "se", "de", "nl", "ch"].indexOf((root.service.mullvad.relay || "").split(" ").slice(-1)[0]))
                    Accessible.name: "Mullvad country"; enabled: root.service.mullvad.account && !root.service.busy
                    onActivated: index => root.service.run("mullvad-location", {country: ["any", "gb", "us", "se", "de", "nl", "ch"][index]})
                }
            }
        }
    }

    PanelLabel { visible: !root.compact; text: "Tailscale"; color: Theme.colors.muted; font.pixelSize: 11 }
    Rectangle {
        visible: !root.compact; width: parent.width; height: tailscaleDetails.height
        radius: 10; color: Theme.colors.surface; border.color: Theme.hairline; clip: true
        Column {
            id: tailscaleDetails; width: parent.width
            Item {
                width: parent.width; height: 44
                PanelLabel { x: 14; anchors.verticalCenter: parent.verticalCenter; text: "Devices Online" }
                PanelLabel { anchors.right: parent.right; anchors.rightMargin: 14; anchors.verticalCenter: parent.verticalCenter
                    text: root.service.tailscale.online ? String(root.service.tailscale.peers) : "—"; color: Theme.colors.muted }
            }
            Rectangle { x: 14; width: parent.width - x; height: 1; color: Theme.hairline }
            Item {
                width: parent.width; height: 44
                PanelLabel { x: 14; anchors.verticalCenter: parent.verticalCenter; text: "Exit Node" }
                DesktopComboBox {
                    id: exitChoice
                    anchors.right: parent.right; anchors.rightMargin: 12; anchors.verticalCenter: parent.verticalCenter
                    width: Math.min(225, parent.width - 105)
                    model: [{id: "", name: "None"}].concat(root.service.tailscale.exitNodes || [])
                    textRole: "name"
                    currentIndex: Math.max(0, model.findIndex(node => node.id === root.service.tailscale.exitNode))
                    Accessible.name: "Tailscale exit node"; enabled: root.service.tailscale.online && !root.service.busy
                    onActivated: index => root.service.run("tailscale-exit", {id: model[index].id})
                }
            }
        }
    }
    PanelLabel {
        visible: !root.compact && root.service.tailscale.exitNodes.length === 0
        width: parent.width; wrapMode: Text.WordWrap
        text: "No exit nodes available. Mullvad exit nodes require a separate Tailscale add-on."
        font.pixelSize: 11; color: Theme.colors.muted
    }
    PanelLabel {
        visible: !root.compact && root.service.mullvad.state === "connected" && root.service.tailscale.online
        width: parent.width; wrapMode: Text.WordWrap
        text: "Both services are connected. Check routing and DNS if internet access changes."
        font.pixelSize: 11; color: Theme.colors.muted
    }
    StatusMessage { width: parent.width; text: root.service.error || root.service.mullvad.error || root.service.tailscale.error || ""; severity: "error" }
    ControlRow { visible: root.compact; width: parent.width; text: "VPN Settings…"; onClicked: root.settingsRequested() }

    Dialog {
        id: loginDialog; modal: true; title: "Sign in to Mullvad"; standardButtons: Dialog.Ok | Dialog.Cancel
        anchors.centerIn: Overlay.overlay
        width: Math.min(380, Overlay.overlay ? Overlay.overlay.width - 32 : 380)
        onAccepted: { root.service.run("mullvad-login", {account: accountNumber.text.trim()}); accountNumber.text = "" }
        onRejected: accountNumber.text = ""
        contentItem: Column {
            spacing: 10
            PanelLabel { width: parent.width; wrapMode: Text.WordWrap
                text: "Enter your 16-digit Mullvad account number. It is sent to the local Mullvad client." }
            TextField { id: accountNumber; width: parent.width; echoMode: TextInput.Password
                inputMethodHints: Qt.ImhDigitsOnly; maximumLength: 16; Accessible.name: "Mullvad account number" }
        }
    }
}

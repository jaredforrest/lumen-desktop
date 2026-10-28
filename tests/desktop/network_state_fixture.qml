import QtQuick
import Quickshell
import "LUMEN_ASSETS" as D

ShellRoot {
    property int step: 0
    property var cases: [
        ["disabled", "unknown", "Wi-Fi Off"],
        ["disconnected", "unknown", "Disconnected"],
        ["associating", "unknown", "Connecting…"],
        ["acquiring", "unknown", "Acquiring address…"],
        ["connected", "portal", "Sign-in required"],
        ["connected", "limited", "Limited Internet"],
        ["connected", "none", "No Internet"],
        ["connected", "full", "Connected"],
        ["connected", "unknown", "Internet status unknown"],
        ["service-unavailable", "unknown", "Wi-Fi service unavailable"]
    ]
    QtObject {
        id: controls
        property var wifi: ({available: true, networks: [], phase: "disabled", connectivity: "unknown"})
        property var bluetoothRadios: ({})
        property bool busy: false
        property string error: ""
        function act() { throw Error("Unexpected network action"); }
    }
    QtObject { id: vpn; property var mullvad: ({state: "disconnected"}); property var tailscale: ({online: false}) }
    QtObject { id: notifications; property bool available: false; property bool dnd: false; property var focusState: ({name: ""}) }
    QtObject { id: stub; property var systemState: controls; property var vpn: vpn; property var notifications: notifications; property string volume: "" }
    D.ControlCentre { id: centre; shell: stub; preferences: ({order: ["wifi"], hidden: []}) }
    D.ConnectivityPanel { id: panel; kind: "Wi-Fi"; controls: controls }
    function wifiTile(item) {
        if (item.label === "Wi-Fi") return item;
        for (const child of item.children || []) {
            const found = wifiTile(child);
            if (found) return found;
        }
        return null;
    }
    function labels(item, out) {
        if (item.text !== undefined) out.push(item.text);
        if (item.subtitle !== undefined) out.push(item.subtitle);
        for (const child of item.children || []) labels(child, out);
        return out;
    }
    Timer {
        interval: 50
        running: true
        repeat: true
        onTriggered: {
            try {
                if (step > 0) {
                    const value = cases[step - 1], expected = value[2];
                    const tile = labels(centre, []), details = labels(panel, []), control = wifiTile(centre);
                    if (!control || control.active !== (value[0] === "connected") || !control.Accessible.name.includes(expected)) throw Error("Wrong accessible state " + expected);
                    if (!tile.includes(expected) || !details.some(text => text === expected || text === "Synthetic · " + expected)) throw Error("Wrong labels " + expected + " " + JSON.stringify(tile) + " " + JSON.stringify(details));
                }
                if (step === cases.length) { console.log("PASS visible connectivity states"); Qt.quit(); return; }
                const value = cases[step++];
                controls.wifi = {available: value[0] !== "service-unavailable", networks: [], name: "Synthetic", phase: value[0], connectivity: value[1]};
            } catch (error) {
                console.error("FAIL " + error);
                Qt.quit();
            }
        }
    }
}

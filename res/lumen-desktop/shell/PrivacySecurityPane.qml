import QtQuick
import Quickshell
import Quickshell.Io

Column {
    id: root
    property bool active: false
    property var facts: ({})
    property string error: ""
    spacing: 12
    function refresh() {
        if (worker.running) return;
        error = "";
        worker.command = ["lumen-privacy-status"];
        worker.running = true;
    }
    onActiveChanged: if (active) refresh()
    Component.onCompleted: if (active) refresh()
    Process {
        id: worker
        property bool received: false
        onRunningChanged: if (running) received = false
        stdout: StdioCollector {
            onStreamFinished: {
                try {
                    const result = JSON.parse(text);
                    if (!result.encryption || !result.secureBoot) throw new Error("Incomplete status");
                    root.facts = result;
                    worker.received = true;
                } catch (e) { root.error = "Security status could not be read."; }
            }
        }
        onExited: (code, status) => {
            if (code !== 0 || !received) {
                root.facts = {};
                root.error = "Security status could not be read.";
            }
        }
    }
    PortalPermissionsPane {width:parent.width;active:root.active}
    Repeater {
        model: [{key:"encryption", name:"Disk encryption"}, {key:"secureBoot", name:"Secure Boot"}, {key:"biometrics", name:"Fingerprint reader"}]
        Rectangle {
            required property var modelData
            readonly property var fact: root.facts[modelData.key] || ({value:worker.running?"Checking…":"Unavailable",detail:""})
            Accessible.role: Accessible.StaticText
            Accessible.name: modelData.name + ". " + fact.value + ". " + fact.detail
            width: root.width
            height: details.y + details.height + 16
            radius: 10; color: Theme.colors.surface; border.color: Theme.hairline
            PanelLabel { id: label; x:14; y:14; width:parent.width-28; text:parent.modelData.name; font.weight:Font.DemiBold }
            PanelLabel { id: value; x:14; y:label.y+label.height+6; width:parent.width-28; text:parent.fact.value }
            PanelLabel { id: details; x:14; y:value.y+value.height+8; width:parent.width-28; text:parent.fact.detail; wrapMode:Text.WordWrap; color:Theme.colors.muted; font.pixelSize:12 }
        }
    }
    PanelLabel { width:parent.width; text:root.error; visible:text!==""; wrapMode:Text.WordWrap; color:Theme.colors.muted }
    ControlRow { width:parent.width; text:worker.running?"Checking…":"Refresh status"; enabled:!worker.running; onClicked:root.refresh() }
    ControlRow { width:parent.width; text:"Passwords…"; detail:"Open Bitwarden"; onClicked:AppLaunch.run(["bitwarden"]) }
}

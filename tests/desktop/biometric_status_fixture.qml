import QtQuick
import Quickshell
import "LUMEN_ASSETS" as D

ShellRoot {
    property int step: 0
    property var cases: [
        ["unknown", "Provider inactive"],
        ["unavailable", "No reader reported"],
        ["reported", "Reader reported"],
        ["unknown", "Unavailable"]
    ]
    D.PrivacySecurityPane { id: pane; width: 360; active: false }
    function find(item) {
        if (item.Accessible && (item.Accessible.name || "").startsWith("Fingerprint reader.")) return item;
        for (const child of item.children || []) {
            const result = find(child);
            if (result) return result;
        }
        return null;
    }
    Timer {
        interval: 80
        running: true
        repeat: true
        onTriggered: {
            try {
                if (step > 0) {
                    const row = find(pane);
                    if (!row || row.Accessible.role !== Accessible.StaticText || !row.Accessible.name.includes(cases[step - 1][1]) || !row.Accessible.name.includes("Not an authentication result")) throw Error("Unreadable or incorrect status");
                    if (row.width !== 360 || row.height <= 0) throw Error("Invalid status layout");
                }
                if (step === cases.length) { console.log("PASS biometric discovery states"); Qt.quit(); return; }
                const value = cases[step++];
                pane.facts = {biometrics: {state: value[0], value: value[1], detail: "Not an authentication result"}};
            } catch (error) {
                console.error("FAIL " + error);
                Qt.quit();
            }
        }
    }
}

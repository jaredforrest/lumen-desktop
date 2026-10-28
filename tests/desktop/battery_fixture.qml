import QtQuick
import Quickshell
import "LUMEN_ASSETS" as D

ShellRoot {
    property int tick: 0
    D.BatteryState { id: battery; freshnessMilliseconds: 50 }
    FloatingWindow {
        visible: true
        implicitWidth: 180
        implicitHeight: 50
        D.BatteryMenuButton { id: indicator; power: battery.status }
    }
    function check(value, message) { if (!value) throw Error(message); }
    Timer {
        interval: 30
        running: true
        repeat: true
        onTriggered: {
            try {
                ++tick;
                if (tick === 1) {
                    check(!indicator.visible, "Unavailable battery was shown");
                    battery.publish({available: true, present: true, percentage: 42, state: "charging", secondsRemaining: 1200});
                    check(indicator.visible && indicator.Accessible.name.includes("42%") && indicator.Accessible.name.includes("Charging"), "Charging state missing");
                    check(indicator.Accessible.description.includes("until full"), "Estimate inaccessible");
                    battery.publish({available: true, present: true, percentage: 4, state: "discharging", secondsRemaining: 60});
                    check(indicator.Accessible.name.includes("Low battery"), "Low state depended only on colour");
                    battery.publish({available: true, present: false, percentage: 99});
                    check(!indicator.visible && battery.status.percentage === null, "Removed battery retained its percentage");
                    battery.publish({available: true, present: true, percentage: 101, state: "charging", secondsRemaining: null});
                    check(!indicator.visible && !battery.status.available, "Malformed percentage accepted");
                    battery.publish({available: true, present: true, percentage: 0, state: "empty", secondsRemaining: null});
                    check(indicator.visible && indicator.Accessible.name.includes("0%"), "Real zero charge hidden");
                }
                if (tick === 4) {
                    check(!indicator.visible && battery.status.percentage === null, "Unresponsive provider retained stale charge");
                    console.log("PASS battery presentation");
                    Qt.quit();
                }
            } catch (error) {
                console.error("FAIL " + error);
                Qt.quit();
            }
        }
    }
}

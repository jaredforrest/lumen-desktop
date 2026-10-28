import QtQuick
import Quickshell
import "LUMEN_ASSETS" as D
import "LUMEN_ASSETS/ClockFormat.js" as Clock

ShellRoot {
    D.StatusStrip { id: strip; order: ["search", "battery", "sound", "focus", "controls"]; available: ({sound: true, search: true, controls: true}); budget: 76 }
    D.ClockSettings {
        service: QtObject {
            property var menuBarPreferences: ({hourCycle: "locale", locale: "", showDate: true, showWeekday: true, showSeconds: false, order: []})
            property bool available: true
            property bool busy: false
            function setMenuBarPreference(k, v) {}
        }
    }
    Timer {
        interval: 100
        running: true
        onTriggered: {
            try {
                if (strip.layout.visible.join(",") !== "sound,controls" || strip.layout.overflow.join(",") !== "search") throw Error("Overflow");
                const midnight = new Date(2026, 9, 19, 0, 5, 9);
                const prefs = {hourCycle: "24", showDate: false, showWeekday: false, showSeconds: true};
                if (Clock.text(midnight, Qt.locale("en_GB"), prefs) !== "00:05:09") throw Error("24h midnight");
                prefs.hourCycle = "12";
                if (Clock.text(midnight, Qt.locale("en_US"), prefs) !== "12:05:09 AM") throw Error("12h midnight");
                prefs.hourCycle = "locale";
                prefs.showSeconds = false;
                if (!Clock.text(midnight, Qt.locale("en_US"), prefs).includes("AM")) throw Error("Locale cycle");
                prefs.showDate = true;
                prefs.showWeekday = true;
                const french = Clock.text(midnight, Qt.locale("fr_FR"), prefs);
                if (!french.includes("lun.") || !french.includes("oct.")) throw Error("Locale date " + french);
                const instants = ["2026-03-29T00:30:00Z", "2026-03-29T01:30:00Z", "2026-10-25T00:30:00Z", "2026-10-25T01:30:00Z"];
                const expected = EXPECTED_CLOCK_VALUES;
                const timePrefs = {hourCycle: "24", showDate: false, showWeekday: false, showSeconds: true};
                for (let i = 0; i < instants.length; i++) {
                    const actual = Clock.text(new Date(instants[i]), Qt.locale("en_GB"), timePrefs);
                    if (actual !== expected[i]) throw Error("Timezone/DST " + instants[i] + " " + actual);
                }
                console.log("PASS status clock");
            } catch (error) {
                console.error("FAIL " + error);
            }
            Qt.quit();
        }
    }
}

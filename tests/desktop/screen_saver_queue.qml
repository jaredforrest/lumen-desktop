import QtQuick
import Quickshell
import Quickshell.Io
import "."

ShellRoot {
    id: test
    property int phase: 0
    property int ticks: 0
    ScreenSaverState { id: service }
    Process {
        id: release
        command: [Quickshell.env("LUMEN_TEST_FIXTURE_BIN"), "screen-saver-release"]
    }
    Timer {
        interval: 50; running: true; repeat: true
        onTriggered: {
            try {
                if (++test.ticks > 120) throw Error("Provider contention deadline");
                if (test.phase === 0) {
                    service.refresh(); test.phase = 1; return;
                }
                if (test.phase === 1) {
                    if (!service.busy) return;
                    service.setValue("enabled", false);
                    service.setValue("idleSeconds", 60);
                    service.setValue("motion", false);
                    service.setValue("idleSeconds", 90);
                    service.refresh();
                    release.running = true; test.phase = 2; return;
                }
                if (test.phase === 2) {
                    if (service.busy) return;
                    if (service.preferences.enabled !== false || service.preferences.idleSeconds !== 90 || service.preferences.motion !== false)
                        throw Error("Explicit settings lost behind status poll: " + JSON.stringify(service.preferences));
                    console.log("QUEUE_PASS"); Qt.quit();
                }
            } catch (error) {
                console.error("QUEUE_FAIL " + error); Qt.exit(1);
            }
        }
    }
}

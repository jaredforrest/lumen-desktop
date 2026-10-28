import QtQuick
import Quickshell
import "MenuLayout.js" as Layout

Timer {
    interval: 50
    running: true
    onTriggered: {
        try {
            if (Layout.fit([40, 50], 90, 30, false) !== 2) throw Error("exact fit");
            if (Layout.fit([40, 50], 89, 30, false) !== 1) throw Error("overflow reservation");
            if (Layout.fit([40, 50], 90, 30, true) !== 1) throw Error("status overflow reservation");
            if (Layout.fit([40], 10, 30, true) !== 0) throw Error("small width");
            if (Layout.fit([], 0, 30, false) !== 0) throw Error("empty model");
            console.log("PASS menu layout partitions");
        } catch (error) {
            console.error("FAIL " + error);
        }
        Qt.quit();
    }
}

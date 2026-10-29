import QtQuick
import Quickshell
import "assets" as D

ShellRoot {
    id: root
    property var cases: __CASES__
    property var records: []
    property int step: 0
    property int stage: 0
    property bool painting: false
    FloatingWindow {
        id: window
        visible: true
        implicitWidth: 500
        implicitHeight: 90
        color: D.Theme.colors.background
        D.SettingsToggle { id: toggle; x: 10; y: 15; width: 480; text: "Private permission" }
    }
    function walk(item) {
        let rows = [item];
        for (const child of item.children || []) rows = rows.concat(walk(child));
        return rows;
    }
    function rgba(color) { return {r:color.r, g:color.g, b:color.b, a:color.a}; }
    Timer {
        interval: 20; repeat: true; running: true
        onTriggered: {
            try {
                if (root.painting) return;
                if (root.step === root.cases.length) {
                    console.log("SWITCH_PAINT_RESULT " + JSON.stringify(root.records)); Qt.quit(); return;
                }
                const entry = root.cases[root.step];
                if (root.stage === 0) {
                    D.Theme.colors = entry.colors;
                    D.Theme.preferences = {reduceMotion:true, increaseContrast:entry.contrast};
                    toggle.checked = entry.checked;
                    root.stage = 1; return;
                }
                const track = root.walk(toggle).find(item => item.width === 32 && item.height === 20);
                const thumb = track ? Array.from(track.children).find(item => item.width === 16) : null;
                if (!thumb) throw Error("Missing actual switch indicator");
                const at = thumb.mapToItem(toggle, 0, 0);
                const indicator = thumb.border.width > 0 ? thumb.border.color : thumb.color;
                const observed = {mode:entry.mode, accent:entry.accent, increaseContrast:entry.contrast,
                    checked:entry.checked, position:thumb.x, fill:root.rgba(track.color),
                    boundary:root.rgba(indicator), thumb:root.rgba(thumb.color), border:thumb.border.width,
                    rect:{x:at.x,y:at.y,width:thumb.width,height:thumb.height},
                    capture:Quickshell.env("LUMEN_SWITCH_CAPTURE_DIR") + "/switch-" + root.step + ".png"};
                if (thumb.x !== (entry.checked ? 14 : 2)) throw Error("Incorrect physical state position");
                root.painting = true;
                if (!toggle.grabToImage(result => {
                    try {
                        if (!result.saveToFile(observed.capture)) throw Error("Actual switch capture failed");
                        root.records = root.records.concat([observed]); root.step++; root.stage = 0; root.painting = false;
                    } catch (error) { console.error("SWITCH_PAINT_FAILURE " + error); Qt.quit(); }
                }, Qt.size(toggle.width*3,toggle.height*3))) throw Error("Capture refused");
            } catch (error) { console.error("SWITCH_PAINT_FAILURE " + error); Qt.quit(); }
        }
    }
}

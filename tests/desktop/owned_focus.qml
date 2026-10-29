import QtQuick
import QtQuick.Controls
import Quickshell
import "assets" as D

ShellRoot {
    id: audit
    property var palettes: __PALETTES__
    property var cases: []
    property var records: []
    property int index: 0
    property int stage: 0
    property bool painting: false
    property var target: null

    FloatingWindow {
        id: window
        visible: true
        implicitWidth: 360
        implicitHeight: 500
        color: "transparent"
        Rectangle {
            id: body
            anchors.fill: parent
            color: D.Theme.colors.background
            D.WindowControls { id: controls; windowActive: true; windowName: "Private fixture" }
            D.FileInfoPanel {
                id: info
                x: 16; y: 60; width: parent.width - 32; height: parent.height - y - 16
                details: ({name:"Private fixture.txt",size:"12 bytes",kind:"Text",location:"Private synthetic directory",permissions:"Read and write"})
            }
            FocusScope { id: neutral; focus: true }
        }
    }
    function walk(item) {
        if (!item) return [];
        let found = [item];
        for (const child of item.children || []) found = found.concat(walk(child));
        if (item.contentItem && !(item.children || []).includes(item.contentItem)) found = found.concat(walk(item.contentItem));
        return found;
    }
    function rect(item, padding) {
        const point = item.mapToItem(body, 0, 0);
        return {x: point.x-padding, y: point.y-padding, width: item.width+2*padding, height: item.height+2*padding};
    }
    Component.onCompleted: {
        for (const mode of ["light", "dark"])
            for (const opaque of [false, true])
                for (const control of ["window-close", "info-general"])
                    cases.push({mode:mode, opaque:opaque, control:control});
    }
    Timer {
        interval: 20; repeat: true; running: true
        onTriggered: {
            try {
                if (audit.painting) return;
                if (audit.index === audit.cases.length) {
                    console.log("OWNED_FOCUS_RESULT " + JSON.stringify(audit.records));
                    Qt.quit(); return;
                }
                const entry = audit.cases[audit.index];
                if (audit.stage === 0) {
                    D.Theme.colors = audit.palettes[entry.mode];
                    D.Theme.preferences = {reduceMotion:true, reduceTransparency:entry.opaque};
                    D.Theme.font = audit.palettes.font;
                    audit.target = entry.control === "window-close"
                        ? audit.walk(controls).find(item => item.index === 0 && item.background)
                        : audit.walk(info).find(item => item.Accessible.name === "General" && item.background);
                    if (!audit.target) throw Error("Production focus control missing");
                    neutral.forceActiveFocus();
                    audit.stage = 1; return;
                }
                if (audit.stage === 1 || audit.stage === 3) {
                    const focused = audit.stage === 3;
                    if (focused && !audit.target.activeFocus) throw Error("Production focus not retained");
                    audit.painting = true;
                    const file = Quickshell.env("LUMEN_FOCUS_CAPTURE_DIR") + "/" + audit.index + (focused ? "-focused" : "-ordinary") + ".png";
                    if (!body.grabToImage(result => {
                        if (!result.saveToFile(file)) {
                            console.error("OWNED_FOCUS_FAIL save " + file);
                            Qt.quit(); return;
                        }
                        if (focused) {
                            audit.records.push({entry:entry, bounds:audit.rect(audit.target, entry.control === "window-close" ? 5 : 0)});
                            audit.index++; audit.stage = 0;
                        } else audit.stage = 2;
                        audit.painting = false;
                    }, Qt.size(window.width, window.height))) throw Error("Frame capture rejected");
                    return;
                }
                audit.target.forceActiveFocus();
                audit.stage = 3;
            } catch (error) {
                console.error("OWNED_FOCUS_FAIL " + error);
                Qt.quit();
            }
        }
    }
}

import QtQuick
import Quickshell
import "assets" as D

ShellRoot {
    id: root
    property var cases: __CASES__
    property int step: 0
    property int stage: 0
    property int messageWidth: 580
    property bool painting: false
    property var records: []
    FloatingWindow {
        id: window
        visible: true
        implicitWidth: 620
        implicitHeight: 460
        color: D.Theme.colors.background
        Rectangle {
            id: frame
            anchors.fill: parent
            color: D.Theme.colors.background
            Column {
                x: 20; y: 20; width: 580; spacing: 12
                Repeater {
                    id: statuses
                    model: ["error", "success", "warning", "info"]
                    D.StatusMessage {
                        required property string modelData
                        width: root.messageWidth
                        text: "The request returned a result"
                        severity: modelData
                    }
                }
                D.SettingsToggle { id: toggle; width: 580; text: "Private permission"; detail: "Switch position also conveys state"; checked: true }
                D.ControlTile { id: radio; width: 580; label: "Private connection"; symbol: "wifi"; toggle: true; unavailable: false; active: true; subtitle: active ? "Connected" : "Disconnected" }
                D.DialogButton { id: disabled; width: 580; text: "Delete private item"; destructive: true; enabled: false; onClicked: { throw Error("Disabled action dispatched"); } }
            }
        }
    }
    function walk(item) {
        let rows = [item];
        for (const child of item.children || []) rows = rows.concat(walk(child));
        return rows;
    }
    function rgba(color) { return {r:color.r, g:color.g, b:color.b, a:color.a}; }
    Timer {
        interval: 30; running: true; repeat: true
        onTriggered: {
            try {
                if (root.painting) return;
                if (root.step === root.cases.length) {
                    console.log("SEMANTIC_PAINT_RESULT " + JSON.stringify(root.records)); Qt.quit(); return;
                }
                const entry = root.cases[root.step];
                if (root.stage === 0) {
                    root.messageWidth = entry.width;
                    D.Theme.colors = entry.colors;
                    D.Theme.preferences = {reduceMotion:true,increaseContrast:entry.contrast};
                    root.stage = 1; return;
                }
                const rows = [];
                for (let index = 0; index < statuses.count; index++) {
                    const item = statuses.itemAt(index);
                    if (!item) return;
                    const at = item.mapToItem(frame, 0, 0);
                    rows.push({severity:item.severity,text:item.text,accessible:item.Accessible.name,
                        visibleTexts:root.walk(item).filter(child => child.visible && child.text !== undefined).map(child => String(child.text)),
                        padding:item.leftPadding,foreground:root.rgba(item.color),background:root.rgba(frame.color),
                        rect:{x:at.x,y:at.y,width:item.width,height:item.height}});
                }
                const track = root.walk(toggle).find(item => item.width === 32 && item.height === 20);
                const thumb = track ? Array.from(track.children).find(item => item.width === 16) : null;
                if (!thumb || thumb.x !== 14 || !toggle.checked) throw Error("Checked switch lacks positional state");
                if (!radio.Accessible.name.includes("Connected") || !radio.Accessible.checked || disabled.enabled) throw Error("State metadata lost");
                root.painting = true;
                const file = Quickshell.env("LUMEN_SEMANTIC_CAPTURE_DIR") + "/" + entry.mode + "-" + entry.contrast + "-" + entry.width + ".png";
                if (!frame.grabToImage(result => {
                    try {
                        if (!result.saveToFile(file)) throw Error("Actual frame capture failed");
                        root.records = root.records.concat([{mode:entry.mode,increaseContrast:entry.contrast,width:entry.width,
                            statuses:rows,capture:file,checkedThumb:thumb.x,radio:radio.Accessible.name,disabled:!disabled.enabled}]);
                        root.step++; root.stage = 0; root.painting = false;
                    } catch (error) { console.error("SEMANTIC_PAINT_FAILURE " + error); Qt.quit(); }
                }, Qt.size(frame.width*2,frame.height*2))) throw Error("Frame capture rejected");
            } catch (error) { console.error("SEMANTIC_PAINT_FAILURE " + error); Qt.quit(); }
        }
    }
}

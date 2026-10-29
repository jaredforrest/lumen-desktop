import QtQuick
import QtQuick.Controls
import QtQuick.Window
import Quickshell
import Quickshell.Io
import "assets/shell" as D
ShellRoot {
    id: audit
    property string popupOwner: ""
    signal dismissPopups(string owner)
    function preferredOutput() { return "HEADLESS-1" }
    function claimPopup(owner) { popupOwner = owner }
    property var palettes: __PALETTES__
    property var owner: null
    D.ForceQuitDialog { id: force; shell: audit }
    D.WindowPairPicker { id: pair; shell: audit }
    FloatingWindow {
        id: displays
        visible: false
        implicitWidth: 340
        implicitHeight: 440
        color: D.Theme.colors.background
        D.FocusFlickable {
            id: displayScroll
            anchors.fill: parent
            anchors.margins: 16
            contentHeight: pane.height
            clip: true
            D.DisplaySettingsPane { id: pane; width: displayScroll.width }
            ScrollBar.vertical: D.DesktopScrollBar {}
        }
    }
    function walk(item, seen) {
        if (!item || seen.includes(item)) return [];
        seen.push(item);
        let result = [item];
        for (const child of item.children || []) result = result.concat(walk(child, seen));
        if (item.contentItem) result = result.concat(walk(item.contentItem, seen));
        return result;
    }
    function focusName(item) {
        while (item) {
            if (item.Accessible && item.Accessible.name) return item.Accessible.name;
            if (item.text) return item.text;
            item = item.parent;
        }
        return "";
    }
    function rect(item) {
        const p = item.mapToItem(owner.contentItem, 0, 0);
        return {x:p.x, y:p.y, width:item.width, height:item.height};
    }
    IpcHandler {
        target: "essentialDialogs"
        function open(kind: string, mode: string): void {
            force.visible = false; pair.visible = false; displays.visible = false;
            D.Theme.colors = audit.palettes[mode];
            D.Theme.preferences = {textScale:1.5, reduceMotion:true};
            if (kind === "Force") {
                audit.owner = force;
                force.applicationName = "Private captured document application with a long descriptive name";
                force.visible = true;
                force.fail("The captured application changed while preparing this request. Close and select the intended application again.");
            } else if (kind === "Pair") {
                audit.owner = pair;
                pair.open(101, "left");
            } else {
                audit.owner = displays;
                displays.visible = true;
            }
            Qt.callLater(() => audit.owner.contentItem.Window.window.requestActivate());
        }
        function focus(name: string): void {
            const item = audit.walk(audit.owner.contentItem, []).find(item =>
                item.Accessible && item.Accessible.name === name);
            if (!item) throw Error("No focus target " + name);
            item.forceActiveFocus();
        }
        function state(): string {
            if (!audit.owner) return "{}";
            const window = audit.owner.contentItem.Window.window;
            if (!window) return JSON.stringify({visible:audit.owner.visible});
            const focused = window.activeFocusItem;
            const items = audit.walk(audit.owner.contentItem, []);
            const labels = items.filter(item => item.text && item.font && item.fontInfo && item.visible)
                .map(item => ({text:item.text, size:item.font.pixelSize, family:item.fontInfo.family,
                               rect:audit.rect(item), implicitHeight:item.implicitHeight, height:item.height}));
            return JSON.stringify({visible:audit.owner.visible, active:window.active,
                focus:audit.focusName(focused), rect:focused ? audit.rect(focused) : null,
                width:audit.owner.width, height:audit.owner.height,
                scale:D.Theme.textScale, phase:force.phase, selected:pair.selected,
                pairReady:pair.canChoose && pair.candidates.length === 1,
                positionX:audit.walk(pane, []).find(item => item.objectName === "displayX").text,
                labels:labels});
        }
    }
}

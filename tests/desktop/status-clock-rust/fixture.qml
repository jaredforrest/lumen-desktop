import QtQuick
import Quickshell
import Quickshell.Io
import "assets/shell" as D
ShellRoot {
    id: fixture
    property int activations: 0
    property var saved: ["battery", "sound", "focus", "search", "controls"]
    FloatingWindow {
        id: window
        title: "Status fixture"
        visible: true
        implicitWidth: 500
        implicitHeight: 160
        color: D.Theme.colors.background
        D.StatusStrip {
            id: strip
            x: 20
            y: 40
            order: fixture.saved
            available: ({battery: true, sound: true, focus: true, search: true, controls: true})
            budget: 222
            onReordered: order => { fixture.saved = order; store.setText(JSON.stringify(order)); }
            onActivated: fixture.activations++
        }
    }
    FileView { id: store; path: Quickshell.env("LUMEN_STATUS_ORDER_PATH") }
    function find(name, item) {
        if (item.objectName === name) return item;
        for (const child of item.children || []) {
            const result = find(name, child);
            if (result) return result;
        }
        return null;
    }
    IpcHandler {
        target: "filesAudit"
        function state(): string { return JSON.stringify({ready: true, order: fixture.saved, visible: strip.layout.visible, overflow: strip.layout.overflow, activations: fixture.activations, width: window.width}); }
        function narrow(): void { strip.budget = 76; }
        function unavailable(): void { strip.available = {sound: true, search: true, controls: true}; }
        function restore(): void { strip.budget = 222; strip.available = {battery: true, sound: true, focus: true, search: true, controls: true}; }
        function geometry(name: string): string {
            const item = fixture.find(name, window.contentItem);
            if (!item) return "null";
            const at = item.mapToItem(window.contentItem, 0, 0);
            return JSON.stringify({x: at.x, y: at.y, width: item.width, height: item.height});
        }
    }
}

import QtQuick
import QtQuick.Window
import Quickshell
import Quickshell.Io
import "assets/shell" as D
ShellRoot {
    D.InputSettingsState { id: service; active: false }
    FloatingWindow {
        id: window
        title: "Input fixture"
        visible: true
        implicitWidth: 420
        implicitHeight: 700
        color: D.Theme.colors.background
        Flickable {
            id: scroll
            anchors.fill: parent
            contentWidth: width
            contentHeight: pane.implicitHeight + 32
            clip: true
            D.InputSettingsPane { id: pane; x: 16; y: 16; width: parent.width - 32; service: service }
        }
    }
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
        function refresh(): void { service.refresh(); }
        function selectKeyboard(): void { pane.selected = service.devices.find(row => row.identity.type === "keyboard").key; }
        function state(): string { return JSON.stringify({ready: service.available && !service.busy, rows: service.devices, selected: pane.device, error: service.error, message: service.message}); }
        function geometry(name: string): string {
            const item = find(name, window.contentItem);
            if (!item) return "null";
            const content = item.mapToItem(pane, 0, 0);
            scroll.contentY = Math.max(0, Math.min(scroll.contentHeight - scroll.height, content.y - 150));
            const at = item.mapToItem(window.contentItem, 0, 0);
            return JSON.stringify({x: at.x, y: at.y, width: item.width, height: item.height});
        }
    }
}

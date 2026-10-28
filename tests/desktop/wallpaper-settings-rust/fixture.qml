import QtQuick
import QtQuick.Window
import QtQuick.Controls
import Quickshell
import Quickshell.Io
import "assets/shell" as D
ShellRoot {
    D.WallpaperSettingsState { id: service; active: false }
    FloatingWindow {
        id: window
        property var overlay: Overlay.overlay
        title: "Wallpaper fixture"
        visible: true
        implicitWidth: 430
        implicitHeight: 760
        color: D.Theme.colors.background
        Flickable {
            id: scroll
            anchors.fill: parent
            contentWidth: width
            contentHeight: pane.implicitHeight + 32
            clip: true
            D.WallpaperSettingsPane { id: pane; x: 16; y: 16; width: parent.width - 32; service: service }
        }
    }
    function find(name, item) {
        if (!item) return null;
        if (item.objectName === name) return item;
        for (const child of item.children || []) {
            const result = find(name, child);
            if (result) return result;
        }
        for (const resource of item.resources || []) {
            if (resource.contentItem) {
                const result = find(name, resource.contentItem);
                if (result) return result;
            }
        }
        return null;
    }
    IpcHandler {
        target: "filesAudit"
        function refresh(): void { service.refresh(); }
        function selectSecond(): void { pane.selected = service.outputs.find(row => row.name === "HEADLESS-2").identity; }
        function state(): string { return JSON.stringify({ready: !service.busy, rows: service.outputs, error: service.error}); }
        function geometry(name: string): string {
            const item = find(name, window.contentItem) || find(name, window.overlay);
            if (!item) return "null";
            const content = item.mapToItem(pane, 0, 0);
            if (!name.startsWith("wallpaper-image-")) scroll.contentY = Math.max(0, Math.min(scroll.contentHeight - scroll.height, content.y - 150));
            const at = item.mapToItem(window.contentItem, 0, 0);
            return JSON.stringify({x: at.x, y: at.y, width: item.width, height: item.height});
        }
    }
}

import QtQuick
import QtQuick.Controls
import QtQuick.Window
import Quickshell
import Quickshell.Io
import Quickshell.Wayland
import "assets/shell" as D
ShellRoot {
    id: root
    property bool inhibit: false
    property int underlyingClicks: 0
    D.ScreenSaverState { id: service; active: true }
    D.ScreenSaverController { id: saver; state: service }
    FloatingWindow {
        id: window
        title: "Saver fixture"
        visible: true
        implicitWidth: 460
        implicitHeight: 600
        color: D.Theme.colors.background
        Column {
            anchors.fill: parent
            anchors.margins: 16
            spacing: 12
            TextField { id: underlying; objectName: "underlying-input"; width: parent.width; placeholderText: "Underlying application input" }
            D.DialogButton { objectName: "underlying-button"; text: "Underlying action"; onClicked: ++root.underlyingClicks }
            D.ScreenSaverPane { width: parent.width; service: service; onPreviewRequested: saver.start(true) }
        }
    }
    IdleInhibitor { window: window; enabled: root.inhibit }
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
        function preview(): void { saver.start(true); }
        function inhibit(value: bool): void { root.inhibit = value; }
        function preference(key: string, value: string): void { service.setValue(key, JSON.parse(value)); }
        function focusInput(): void { underlying.forceActiveFocus(); }
        function state(): string { return JSON.stringify({ready: !service.busy, preferences: service.preferences, permitted: service.permitted, lock: service.lock, showing: saver.showing, reason: saver.reason, text: underlying.text, clicks: root.underlyingClicks, error: service.error}); }
        function geometry(name: string): string {
            const item = find(name, window.contentItem);
            if (!item) return "null";
            const at = item.mapToItem(window.contentItem, 0, 0);
            return JSON.stringify({x: at.x, y: at.y, width: item.width, height: item.height});
        }
    }
}

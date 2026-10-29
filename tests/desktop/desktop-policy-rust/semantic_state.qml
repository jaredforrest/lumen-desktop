import QtQuick
import QtQuick.Controls
import Quickshell
import Quickshell.Io
import "assets/shell" as D

ShellRoot {
    id: root
    property int deleted: 0
    property var pointerReceipts: []
    FloatingWindow {
        id: window
        visible: true
        title: "Private semantic states"
        implicitWidth: 600
        implicitHeight: 360
        color: D.Theme.colors.background
        MouseArea {
            anchors.fill: parent
            z: -1
            onClicked: mouse => root.pointerReceipts = root.pointerReceipts.concat({x: mouse.x, y: mouse.y})
        }
        Column {
            x: 20; y: 20; width: 560; spacing: 16
            D.StatusMessage { id: status; width: parent.width; text: "The request returned a result"; severity: "error" }
            D.SettingsToggle { id: toggle; width: parent.width; text: "Private permission"; checked: false }
            D.ControlTile { id: radio; width: parent.width; label: "Private connection"; symbol: "wifi"; toggle: true; unavailable: false; active: false; subtitle: active ? "Connected" : "Disconnected"; onClicked: active = !active }
            D.DialogButton { id: remove; width: parent.width; text: "Delete private item"; destructive: true; enabled: false; defaultAction: false; onClicked: root.deleted++ }
        }
    }
    function walk(item) {
        let rows = [item];
        for (const child of item.children || []) rows = rows.concat(walk(child));
        return rows;
    }
    IpcHandler {
        target: "semantic"
        function focus(name: string): bool {
            const item = name === "toggle" ? toggle : radio;
            item.forceActiveFocus();
            return item.activeFocus;
        }
        function severity(value: string): void { status.severity = value; }
        function availability(value: bool): void { radio.unavailable = !value; }
        function state(): string {
            const track = root.walk(toggle).find(item => item.width === 32 && item.height === 20);
            const thumb = track ? Array.from(track.children).find(item => item.width === 16) : null;
            const position = radio.mapToItem(window.contentItem, 0, 0);
            return JSON.stringify({
                dark: D.Theme.dark, contrast: D.Theme.increaseContrast,
                visible: window.visible, checked: toggle.checked, thumb: thumb ? thumb.x : null,
                toggleFocus: toggle.activeFocus, radioFocus: radio.activeFocus, radioEnabled: radio.enabled,
                pointerReceipts: root.pointerReceipts,
                radioRect: {x: position.x, y: position.y, width: radio.width, height: radio.height},
                active: radio.active, unavailable: radio.unavailable, radioName: radio.Accessible.name,
                deleted: root.deleted, statusName: status.Accessible.name, severity: status.severity,
                statusTexts: root.walk(status).filter(item => item.visible && item.text !== undefined).map(item => item.text),
                rect: {width: window.width, height: window.height}
            });
        }
    }
}

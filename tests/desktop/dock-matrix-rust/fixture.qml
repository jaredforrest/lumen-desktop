import QtQuick
import Quickshell
import Quickshell.Io
import "@SHELL_ASSETS@" as Desktop

ShellRoot {
    id: mock
    property string popupOwner: ""
    property bool dockPreferencesBusy: false
    property var currentDate: new Date()
    property int activated: 0
    property var referenceDock: [
        {key: "a", entry: {id: "a", name: "A", icon: "utilities-terminal"}, canvas: 68, image: ""},
        {key: "b", entry: {id: "b", name: "B", icon: "utilities-terminal"}, canvas: 68, image: ""},
        {key: "downloads", name: "Downloads", action: "downloads", canvas: 68, image: ""}
    ]
    signal requestDockFocus(string name)
    signal dismissPopups(string owner)
    function claimPopup(owner) { popupOwner = owner; dismissPopups(owner); }
    function dockWindows(item) { return []; }
    function dockLabel(item) { return item.name || item.entry.name; }
    function activateDock(item) { activated++; }
    function isDockPinned(entry) { return true; }
    function canConfigureDockEntry(entry) { return true; }
    function windowsFor(entry) { return []; }
    function updateDockPin(entry, action) {}
    function reorderDockPin(a, b) {}

    Desktop.Dock { id: dock; screen: Quickshell.screens[0]; shell: mock }
    IpcHandler {
        target: "fixture"
        function configure(position: string, autoHide: bool, size: int): void {
            Desktop.Theme.preferences = Object.assign({}, Desktop.Theme.preferences, {
                dockPosition: position, dockAutoHide: autoHide, dockSize: size, reduceMotion: true
            });
        }
        function status(): string {
            return JSON.stringify({
                width: dock.width, height: dock.height, slot: dock.slot, iconSize: dock.iconSize,
                vertical: dock.vertical, position: dock.position, revealed: dock.revealed,
                hovered: dock.hovered, autoHide: dock.autoHide, zone: dock.exclusiveZone,
                activated: mock.activated
            });
        }
    }
}

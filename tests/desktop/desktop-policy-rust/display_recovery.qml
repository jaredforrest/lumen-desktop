import QtQuick
import QtQuick.Window
import Quickshell
import Quickshell.Io
import "assets/shell" as D

ShellRoot {
    id: stub
    D.DisplaySpacesState { id: policy }
    property alias displaySpaces: policy
    property string popupOwner: ""
    property bool dockPreferencesBusy: false
    property var currentDate: new Date()
    property string aliasError: ""
    property string applicationCommandError: ""
    property var referenceDock: [{key: "synthetic", entry: {id: "synthetic", name: "Synthetic", icon: "utilities-terminal"}, canvas: 68, image: ""}]
    signal requestDockFocus(string name)
    signal dismissPopups(string owner)
    function claimPopup(owner) { popupOwner = owner; dismissPopups(owner); }
    function dockWindows(item) { return []; }
    function dockLabel(item) { return item.entry.name; }
    function activateDock(item) {}
    function isDockPinned(entry) { return true; }
    function canConfigureDockEntry(entry) { return true; }
    function windowsFor(entry) { return []; }
    function updateDockPin(entry, action) {}
    function reorderDockPin(a, b) {}
    Variants { id: docks; model: Quickshell.screens
        D.Dock { required property var modelData; screen: modelData; shell: stub }
    }
    FloatingWindow { id: preferences; visible: false; title: "Display Space Preferences"
        implicitWidth: 520; implicitHeight: 520
        D.SpaceSettingsPane { id: pane; anchors.fill: parent; anchors.margins: 16; policy: stub.displaySpaces }
    }
    function find(item, name) {
        if (item.objectName === name) return item;
        for (const child of item.children || []) {
            const found = find(child, name);
            if (found) return found;
        }
        return null;
    }
    IpcHandler { target: "displaySpaceRecovery"
        function status(): string {
            return JSON.stringify({policy: policy.state, accepted: policy.lastAccepted,
                routing: policy.routing, docks: docks.instances.map(d => ({name: d.outputName,
                    visible: d.visible, keyboard: d.keyboardMode,
                    active: d.contentItem.Window.window ? d.contentItem.Window.window.active : false}))});
        }
        function focus(): void { stub.requestDockFocus("HEADLESS-2"); }
        function pause(): void { policy.observer.signal(19); }
        function resume(): void { policy.observer.signal(18); }
        function terminate(): void { policy.observer.signal(15); }
        function preferencesOpen(): void { preferences.visible = true; }
        function preferenceControl(name: string): bool {
            const item = stub.find(pane, name);
            if (!item) return false;
            item.forceActiveFocus();
            return item.activeFocus;
        }
        function preferencesClose(): void { preferences.visible = false; }
    }
}

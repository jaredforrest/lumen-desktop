import QtQuick
import Quickshell
import Quickshell.Io
import "lumen-desktop/shell" as D

ShellRoot {
    QtObject {
        id: stub
        property string popupOwner: ""
        signal dismissPopups(string owner)
        function claimPopup(owner) { popupOwner = owner; dismissPopups(owner); }
        function pairWindow(id, side) { picker.open(id, side); }
    }
    D.WindowLayoutMenu { id: menu; shell: stub }
    D.WindowPairPicker { id: picker; shell: stub }
    function findText(item, text) {
        if (item && item.text === text) return item;
        for (const child of (item && item.children || [])) {
            const found = findText(child, text);
            if (found) return found;
        }
        return null;
    }
    IpcHandler {
        target: "pairTest"
        function route(id: string, side: string): void {
            menu.target = Number(id);
            menu.updateCapabilities(JSON.stringify({native_window_layouts: true, native_paired_layouts: true}));
            menu.apply(menu.choices.findIndex(choice => choice.action === "pair-" + side));
        }
        function select(id: string): void {
            picker.selected = picker.candidates.findIndex(item => item.id === Number(id));
        }
        function click(text: string): bool {
            const button = findText(picker.contentItem, text);
            if (button && button.enabled) { button.clicked(); return true; }
            return false;
        }
        function status(): string {
            return JSON.stringify({
                visible: picker.visible, busy: picker.busy, source: picker.source,
                current: picker.current, side: picker.side, error: picker.error
            });
        }
    }
}

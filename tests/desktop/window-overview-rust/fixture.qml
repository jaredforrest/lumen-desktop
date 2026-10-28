import QtQuick
import Quickshell
import Quickshell.Io
import "lumen-desktop/shell" as D

ShellRoot {
    QtObject { id: stub; function claimPopup(owner) {} }
    D.WindowOverview { id: overview; shell: stub }
    function findText(item, text) {
        if (item && item.text === text) return item;
        for (const child of (item && item.children || [])) {
            const found = findText(child, text);
            if (found) return found;
        }
        return null;
    }
    function findName(item, name) {
        if (item && item.Accessible && item.Accessible.name === name) return item;
        for (const child of (item && item.children || [])) {
            const found = findName(child, name);
            if (found) return found;
        }
        return null;
    }
    function moveButton(item) {
        if (item && item.text === "Move Window") return item;
        for (const child of (item && item.children || [])) {
            const found = moveButton(child);
            if (found) return found;
        }
        return null;
    }
    IpcHandler {
        target: "overviewTest"
        function capture(id: string): void { overview.showForWindow(Number(id)); }
        function refresh(): void { overview.send({action: "status"}); }
        function status(): string {
            return JSON.stringify({
                visible: overview.visible, busy: overview.busy,
                dragged: overview.draggedId, selected: overview.selected,
                current: overview.current ? overview.current.id : null,
                error: overview.error,
                moveEnabled: !!moveButton(overview.contentItem) && moveButton(overview.contentItem).enabled,
                assignEnabled: findText(overview.contentItem, "Assign Application").enabled,
                orderEnabled: findText(overview.contentItem, "Recent desktop order").enabled,
                outputEnabled: findName(overview.contentItem, "Destination display").enabled,
                desktopEnabled: !!findText(overview.contentItem, "Desktop 1") && findText(overview.contentItem, "Desktop 1").enabled
            });
        }
        function move(): bool {
            const button = moveButton(overview.contentItem);
            if (button && button.enabled) { button.clicked(); return true; }
            return false;
        }
        function activate(): void { overview.activate(); }
        function activateAndRefresh(): void {
            overview.activate();
            overview.send({action: "status"});
        }
    }
}

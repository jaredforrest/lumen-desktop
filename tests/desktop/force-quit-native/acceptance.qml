import QtQuick
import Quickshell
import Quickshell.Io
import "@SHELL_URI@" as Desktop
ShellRoot {
    id: fixture
    property string popupOwner: ""
    signal dismissPopups(string owner)
    function preferredOutput() { return "HEADLESS-1"; }
    function claimPopup(owner) { popupOwner = owner; dismissPopups(owner); }
    Desktop.ForceQuitDialog { id: dialog; shell: fixture }
    function focused(item) {
        if (item.activeFocus && typeof item.text === "string") return item.text;
        for (let i = 0; i < (item.children || []).length; ++i) {
            const found = focused(item.children[i]);
            if (found) return found;
        }
        return "";
    }
    IpcHandler {
        target: "confirmationProof"
        function openConfirmation(appId: string, title: string): void {
            dialog.show("Disposable document", [{appId: appId, title: title}]);
        }
        function status(): string {
            return JSON.stringify({visible: dialog.visible, phase: dialog.phase,
                error: dialog.error, windowCount: dialog.windowCount,
                focus: fixture.focused(dialog.contentItem)});
        }
    }
}

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
    function labels(item, seen) {
        seen=seen || [];
        if (!item || seen.includes(item)) return [];
        seen.push(item);
        let rows=[];
        if (typeof item.text === "string" && item.font && item.visible)
            rows.push({text:item.text,size:item.font.pixelSize,width:item.width,height:item.height,implicitHeight:item.implicitHeight});
        for (const child of item.children || []) rows=rows.concat(labels(child,seen));
        if (item.contentItem) rows=rows.concat(labels(item.contentItem,seen));
        return rows;
    }
    IpcHandler {
        target: "confirmationProof"
        function openConfirmation(appId: string, title: string): void {
            Desktop.Theme.preferences={textScale:1.5,reduceMotion:true};
            dialog.show("Disposable document", [{appId: appId, title: title}]);
        }
        function status(): string {
            return JSON.stringify({visible: dialog.visible, phase: dialog.phase,
                error: dialog.error, windowCount: dialog.windowCount,
                focus: fixture.focused(dialog.contentItem),scale:Desktop.Theme.textScale,
                width:dialog.width,height:dialog.height,labels:fixture.labels(dialog.contentItem)});
        }
    }
}

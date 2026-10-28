import QtQuick
import Quickshell
import Quickshell.Io
import Quickshell.Wayland
import "source" as Desktop

PanelWindow {
    id: window
    anchors { left: true; right: true; top: true; bottom: true }
    color: "#686870"
    WlrLayershell.keyboardFocus: WlrKeyboardFocus.Exclusive
    Desktop.CalendarCard {
        id: card
        today: new Date(2026, 9, 8)
        width: 344
        height: implicitHeight
        x: (parent.width - width * scale) / 2
        y: 38
        transformOrigin: Item.TopLeft
        scale: Math.min(1, (parent.height - 110) / height)
    }
    function walk(item) {
        let out = [item];
        for (const child of item.children || []) out = out.concat(walk(child));
        return out;
    }
    function field(name) { return walk(card).find(n => n.placeholderText === name); }
    function button(name) { return walk(card).find(n => n.text === name && typeof n.clicked === 'function'); }
    function bounds(item) {
        if (!item) return null;
        const pos = item.mapToItem(null, 0, 0);
        return {x:pos.x, y:pos.y, width:item.width * card.scale, height:item.height * card.scale, visible:item.visible};
    }
    IpcHandler {
        target: "calendarTest"
        function status(): string {
            return JSON.stringify({loaded:card.loaded, busy:card.busy, editing:card.editing, error:card.error,
                title:window.field("Event title").text, notes:window.field("Notes").text,
                save:window.bounds(window.button("Save")), cancel:window.bounds(window.button("Cancel")),
                notesBounds:window.bounds(window.field("Notes")), height:window.height, scale:card.scale});
        }
        function edit(existing: bool): void { card.startEdit(existing ? card.events[0] : null); }
        function save(): void { card.saveEvent(); }
        function cancel(): void { window.button("Cancel").clicked(); }
        function notes(): void { window.field("Notes").forceActiveFocus(); }
    }
}

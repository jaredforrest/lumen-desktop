import QtQuick
import Quickshell
import Quickshell.Io
import "source" as Desktop

ShellRoot {
    QtObject {
        id: stub
        property date currentDate: new Date(2026, 9, 8, 12, 30)
        function claimPopup(owner) {}
        function focusOwnedWindow(title) {}
    }
    Desktop.CalendarWindow { id: window; shell: stub; visible: true }
    IpcHandler {
        target: "calendarTest"
        function status(): string {
            return JSON.stringify({loaded:window.calendarState.loaded, visible:window.visible, view:window.view,
                month:window.calendarState.iso(window.month), selected:window.calendarState.iso(window.selected),
                events:window.calendarState.monthEvents.length, editing:window.calendarState.editing});
        }
        function view(name: string): void { window.view = name; }
        function step(delta: int): void { window.step(delta); }
        function edit(): void { window.create(window.selected); }
        function closeEditor(): void { window.inspector = false; window.calendarState.editing = false; }
    }
}

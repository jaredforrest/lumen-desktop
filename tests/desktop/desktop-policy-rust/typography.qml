import QtQuick
import QtQuick.Controls
import QtQuick.Window
import Quickshell
import Quickshell.Io
import "assets/shell" as D

ShellRoot {
    id: audit
    property string cancellationResult: ""
    QtObject { id: exported; property var state: ({appName: "", title: "", windowId: 0}) }
    QtObject {
        id: stub
        property date currentDate: new Date(2026, 9, 20)
        property int focusEpoch: 0
        property var windows: []
        property var exportedMenu: exported
        function activeFiles(window) {}
        function focusWindow(window) {}
        function claimPopup(name) {}
        function focusOwnedWindow(name) {}
        function openSettings(name) {}
    }
    D.FilesState {
        id: service
        persistViewPreferences: false
        path: Quickshell.env("LUMEN_FIXTURE_DOCUMENTS")
        viewMode: "list"
    }
    D.FilesWindow { id: files; shell: stub; service: service }
    D.CalendarWindow { id: calendar; shell: stub }
    Component.onCompleted: calendar.calendarState.active = false

    function walk(item, seen) {
        if (!item || seen.includes(item)) return [];
        seen.push(item);
        let found = [item];
        for (const child of Array.from(item.children || []).concat(Array.from(item.resources || [])))
            found = found.concat(walk(child, seen));
        if (item.contentItem) found = found.concat(walk(item.contentItem, seen));
        return found;
    }
    function shown(item, stop) {
        while (item && item !== stop) {
            if (!item.visible) return false;
            item = item.parent;
        }
        return item === stop;
    }
    function field(window, hint) {
        return walk(window.contentItem, []).find(item => item.placeholderText === hint && shown(item, window.contentItem));
    }
    function state(name) {
        const window = name === "Files" ? files : calendar;
        const native = window.contentItem.Window.window;
        const focus = native ? native.activeFocusItem : null;
        const search = field(window, name === "Files" ? "Filter folder or tag:red" : "Search");
        const entry = field(window, "Folder path");
        const day = focus && focus.day !== undefined ? focus : null;
        const viewport = walk(window.contentItem, []).find(item => item.objectName === "calendar-content-viewport");
        const point = day && viewport ? day.mapToItem(viewport, 0, 0) : null;
        const primary = walk(window.contentItem, []).find(item => item.text === (name === "Files" ? "An ordinary document.txt" : "Calendars") && item.fontInfo);
        return {
            title: window.title, width: window.width, height: window.height,
            active: !!native && native.active, textScale: D.Theme.textScale,
            configuredFamily: D.Theme.font,
            resolvedFamily: primary ? primary.fontInfo.family : "",
            focus: focus ? {type: String(focus), objectName: focus.objectName || "", hint: focus.placeholderText || "", date: day ? calendar.calendarState.iso(day.day) : null} : null,
            search: search ? {text: search.text, focused: search.activeFocus} : null,
            entry: entry ? {focused: entry.activeFocus, text: entry.text} : null,
            editorOpen: walk(window.contentItem, []).some(item => item.action === "goto" && item.opened === true),
            rows: service.rows.map(row => row.name), path: service.path,
            selected: calendar.calendarState.iso(calendar.selected),
            weeks: calendar.calendarState.weekCount,
            inspector: calendar.inspector,
            dayRevealed: point ? point.x >= -1 && point.y >= -1 && point.x + day.width <= viewport.width + 1 && point.y + day.height <= viewport.height + 1 : null
        };
    }
    IpcHandler {
        target: "ownedText"
        function ready(): bool { return true; }
        function showConsumer(name: string): void {
            files.visible = name === "Files";
            calendar.visible = name === "Calendar";
            if (name === "Files") files.navigate(service.path);
            else {
                calendar.calendarState.active = false;
                calendar.calendarState.loaded = true;
                calendar.calendarState.providers = [{name: "Private", calendars: [{name: "Calendar collection"}]}];
            }
            Qt.callLater(() => {
                const window = name === "Files" ? files : calendar;
                window.fitScreen();
                window.contentItem.Window.window.requestActivate();
                const content = window.contentItem.children.find(item => item.visible && item.width === window.width && item.height === window.height);
                if (content) content.forceActiveFocus();
            });
        }
        function status(name: string): string { return JSON.stringify(audit.state(name)); }
        function focusContent(name: string): void {
            const window = name === "Files" ? files : calendar;
            const content = name === "Files"
                ? walk(window.contentItem, []).find(item => item.objectName === "files-keyboard-content")
                : window.contentItem.children.find(item => item.visible && item.width === window.width && item.height === window.height);
            if (content) content.forceActiveFocus();
        }
        function focusLastDay(): void {
            const days = walk(calendar.contentItem, []).filter(item => item.day !== undefined && item.inMonth !== undefined);
            days[days.length - 1].forceActiveFocus();
        }
        function resetCalendar(): void { calendar.view = "Month"; calendar.choose(new Date(2026, 9, 31)); }
        function inspect(): void { calendar.inspector = true; }
        function cancellation(): string { return audit.cancellationResult; }
        function runFocusCancellation(kind: string): void {
            audit.cancellationResult = "pending";
            const origin = walk(calendar.contentItem, []).find(item => item.day !== undefined && item.inMonth === true);
            origin.forceActiveFocus();
            calendar.chooseAndFocusDay(new Date(2026, 9, 23));
            let target;
            if (kind === "view") {
                calendar.view = "Year";
                target = field(calendar, "Search");
                target.forceActiveFocus();
            } else if (kind === "window") {
                calendar.visible = false;
                files.visible = true;
                files.contentItem.Window.window.requestActivate();
                target = field(files, "Filter folder or tag:red");
                target.forceActiveFocus();
            } else {
                target = field(calendar, "Search");
                target.forceActiveFocus();
            }
            Qt.callLater(() => audit.cancellationResult = (target.activeFocus || (kind === "window" && !calendar.visible && target.focus)) ? "passed" : "failed");
        }
    }
}

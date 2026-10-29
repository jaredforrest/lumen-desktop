import QtQuick
import Quickshell
import Quickshell.Io
import "assets/shell" as D

ShellRoot {
    id: fixture
    property string kind: Quickshell.env("OWNED_FIXTURE_KIND") || "Settings"
    D.ScreenSaverState { id: saver; active: false }
    QtObject { id: stub
        property var screenSaverPreferences: saver
        property string volume: "Volume: 0.5"
        property var audioState: ({available: false, muted: false, outputs: [], inputs: [], streams: [], inputAvailable: false, sink: null, source: null})
        property var notifications: ({dnd: false, available: false, applications: [], policies: {}, history: [], error: ""})
        property var search: ({available: false, scanning: false, indexed: 0})
        property var shortcuts: ({available: false, bindings: [], error: ""})
        property date currentDate: new Date(2026, 9, 20)
        function claimPopup(name) {}
        function focusOwnedWindow(name) {}
        function openSettings(name) {}
    }
    QtObject { id: service
        property var preferences: D.Theme.preferences
        property var windowsPreferences: D.Theme.windowsPreferences
        property var menuBarPreferences: D.Theme.menuBarPreferences
        property var desktopPreferences: D.Theme.desktopPreferences
        property var notificationsPreferences: ({preview: "when_unlocked"})
        property var searchPreferences: ({disabledProviders: []})
        property var appearance: ({})
        property var system: ({system: "Disposable", desktop: "Disposable"})
        property bool available: true
        property bool busy: false
        property bool applying: false
        property string error: ""
        property string warning: ""
        function refresh() {}
        function setPreference(key, value) {}
        function setWindowPreference(key, value) {}
        function setMenuBarPreference(key, value) {}
        function setDesktopPreference(key, value) {}
        function setSearchPreference(key, value) {}
        function setNotificationPreference(key, value) {}
    }
    D.SettingsWindow { id: settings; shell: stub; service: service; page: "Appearance" }
    D.CalendarWindow { id: calendar; shell: stub }
    property var main: kind === "Settings" ? settings : calendar
    Component.onCompleted: {
        calendar.calendarState.active = false;
        calendar.calendarState.loaded = true;
        main.visible = true;
    }
    function visit(item) {
        if (item.ownerWindow === main && item.completedRequests !== undefined) return item;
        for (const child of item.children || []) {
            const found = visit(child);
            if (found) return found;
        }
        return null;
    }
    function findNamed(item, name) {
        if (item.objectName === name) return item;
        for (const child of item.children || []) {
            const found = findNamed(child, name);
            if (found) return found;
        }
        return null;
    }
    function footerBounds() {
        const names = kind === "Settings" ? ["settings-navigation-viewport", "settings-content-scroll"] :
            ["calendar-sidebar-viewport", "calendar-content-viewport"];
        const footer = findNamed(main.contentItem, "owned-window-placement-footer");
        if (!footer) return null;
        const point = footer.mapToItem(main.contentItem, 0, 0);
        return {y: point.y, height: footer.height,
            viewports: names.map(name => {
                const item = findNamed(main.contentItem, name);
                const at = item.mapToItem(main.contentItem, 0, 0);
                return {name: name, bottom: at.y + item.height};
            })};
    }
    IpcHandler { target: "ownedComponentsAudit"
        function status(): string {
            const fit = fixture.visit(fixture.main.contentItem);
            return JSON.stringify({title: fixture.main.title, visible: fixture.main.visible,
                screen: fixture.main.screen ? fixture.main.screen.name : "",
                footer: fixture.footerBounds(), fit: fit ? {
                    completed: fit.completedRequests, busy: fit.busy, error: fit.error,
                    persistenceSupported: fit.persistenceSupported,
                    persistenceStatus: fit.persistenceStatus} : null});
        }
    }
}

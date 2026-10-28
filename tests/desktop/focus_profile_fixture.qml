import QtQuick
import Quickshell
import "LUMEN_ASSETS" as D

ShellRoot {
    QtObject {
        id: service
        property var focusState: ({profiles: [{id: "work", name: "Work", allowedApps: [], allowCritical: false, schedules: [{days: [0], start: 540, end: 1020}]}]})
        property var applications: [{key: "chat", name: "Chat"}]
        property var last: null
        function command(data) { last = data; focusState = {profiles: [data.profile]}; }
    }
    D.FocusProfileEditor { id: editor; service: service; profileId: "work" }
    Timer {
        interval: 100
        running: true
        onTriggered: {
            try {
                if (editor.profile.name !== "Work") throw Error("profile not loaded");
                editor.update("allowedApps", ["chat"]);
                if (service.last.profile.allowedApps[0] !== "chat") throw Error("exception not sent");
                editor.schedule(0, "end", 600);
                if (service.last.profile.schedules[0].end !== 600) throw Error("schedule not sent");
                if (service.last.profile.name !== "Work" || service.last.profile.allowedApps[0] !== "chat") throw Error("sibling policy lost");
                console.log("PASS Focus editor policy updates");
            } catch (error) {
                console.error("FAIL " + error);
            }
            Qt.quit();
        }
    }
}

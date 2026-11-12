pragma Singleton
import QtQuick
import Quickshell
QtObject {
    // Several panels show the same calendars. Report a problem once per session,
    // rather than producing a notification for every widget and refresh.
    property var reported:({})
    function report(problem) {
        if(!problem || reported[problem])return;
        reported[problem]=true;
        Quickshell.execDetached(["gdbus","call","--session",
            "--dest","org.freedesktop.Notifications","--object-path","/org/freedesktop/Notifications",
            "--method","org.freedesktop.Notifications.Notify",
            "Calendar","0","x-office-calendar","Calendar needs attention",
            "Calendar refresh failed. Saved events are still available. Open Calendar for details.",
            "[]","{}","-1"]);
    }
}

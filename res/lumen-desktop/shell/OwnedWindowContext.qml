import QtQml

QtObject {
    id: root
    // Object references supplied by our windows, never inferred from titles.
    property var keyWindow: null
    property var mainWindow: null
    property string applicationId: ""
    property string keyRole: ""
    property int revision: 0
    function focus(window, main, appId, role) {
        if (!window || !main || !main.visible || !appId || !["main", "utility"].includes(role)) return;
        keyWindow = window;
        mainWindow = main;
        applicationId = appId;
        keyRole = role;
        revision++;
    }
    function leave(window) {
        if (keyWindow !== window) return;
        keyWindow = null;
        keyRole = "";
        revision++;
    }
    function close(window) {
        if (keyWindow === window) { keyWindow = null; keyRole = ""; }
        if (mainWindow === window) { mainWindow = null; applicationId = ""; }
        revision++;
    }
}

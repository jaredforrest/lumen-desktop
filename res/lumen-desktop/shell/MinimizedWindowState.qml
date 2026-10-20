import QtQuick
QtObject {
    id: root
    property bool enabled: false
    property var windows: []
    property var entryForWindow: window => null
    property var records: []
    property int nextToken: 1
    function reconcile() {
        records=windows.map(window=>records.find(record=>record.window===window) || {window:window,key:"minimized-window:"+(nextToken++)});
    }
    onWindowsChanged:reconcile()
    Component.onCompleted:reconcile()
    readonly property var items:!enabled?[]:records.filter(record=>windows.includes(record.window) && record.window.minimized).map(record=>({
        key:record.key,action:"minimized-window",section:"documents",name:record.window.title || "Untitled window",
        window:record.window,lease:record,entry:entryForWindow(record.window) || {id:"",name:"Application",icon:"application-x-executable"},canvas:68,bottom:13,image:""
    }))
    function valid(item) {
        return enabled && item && item.action==="minimized-window" && records.includes(item.lease) && item.key===item.lease.key &&
            item.window===item.lease.window && windows.includes(item.window) && item.window.minimized;
    }
    function activate(item) {
        if(!valid(item))return false;
        // The retained protocol object is the capability; names are labels only.
        item.window.activate();
        return true;
    }
}

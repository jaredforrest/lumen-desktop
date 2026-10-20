import QtQuick
import QtQml.Models
QtObject {
    id: root
    property bool enabled: false
    property var windows: []
    property var entryForWindow: window => null
    property var records: []
    property int nextToken: 1
    function reconcile(retiredWindow) {
        const previous=records;
        records=enabled?windows.filter(window=>window && window.minimized).map(window=>
            previous.find(record=>record.window===window && window!==retiredWindow) || {window:window,key:"minimized-window:"+(nextToken++)}):[];
    }
    // Observe transitions directly: a false/true cycle in one turn must retire intent.
    property Instantiator observers:Instantiator {
        model:root.windows
        delegate:QtObject {
            id:observer
            required property var modelData
            property Connections changes:Connections {
                target:observer.modelData
                function onMinimizedChanged(){root.reconcile(observer.modelData);}
            }
        }
    }
    onWindowsChanged:reconcile()
    onEnabledChanged:{records=[];reconcile();}
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

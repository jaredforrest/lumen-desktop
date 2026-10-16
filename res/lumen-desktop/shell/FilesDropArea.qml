import QtQuick

// One native drop receiver survives navigation and delegate replacement.
DropArea {
    id: root
    required property string currentFolder
    property var targets: []
    property var hoverTarget: null
    property string destination: ""
    signal openFolder(string path)
    signal filesDropped(var urls, string destination)

    function registerTarget(item) { targets = targets.concat([item]); }
    function unregisterTarget(item) {
        if (hoverTarget === item) { spring.stop(); hoverTarget = null; }
        targets = targets.filter(t => t !== item);
    }
    function locate(x, y) {
        let found = null;
        for (const item of targets) {
            if (!item || !item.visible || !item.enabled) continue;
            const p = item.mapFromItem(root, x, y);
            if (p.x >= 0 && p.y >= 0 && p.x < item.width && p.y < item.height) {
                let clipped = false;
                for (let parent = item.parent; parent; parent = parent.parent) {
                    if (!parent.clip) continue;
                    const q = parent.mapFromItem(root, x, y);
                    if (q.x < 0 || q.y < 0 || q.x >= parent.width || q.y >= parent.height) {
                        clipped = true; break;
                    }
                }
                if (clipped) continue;
                found = item; break;
            }
        }
        destination = found ? found.folder : currentFolder;
        if (hoverTarget !== found) {
            spring.stop(); hoverTarget = found;
            if (found && found.folder !== currentFolder) spring.start();
        }
    }
    function clear() { spring.stop(); hoverTarget = null; destination = ""; }
    onEntered: drag => { drag.accepted = drag.hasUrls; if (drag.hasUrls) locate(drag.x, drag.y); }
    onPositionChanged: drag => { if (drag.hasUrls) locate(drag.x, drag.y); }
    onExited: clear()
    onDropped: drop => {
        if (drop.hasUrls) {
            const folder = destination || currentFolder;
            filesDropped(drop.urls.map(u => u.toString()), folder);
            drop.accept(Qt.CopyAction);
        }
        clear();
    }
    Timer {
        id: spring; interval: 850
        onTriggered: {
            if (!root.containsDrag || !root.hoverTarget) return;
            const folder = root.hoverTarget.folder;
            root.hoverTarget = null;
            // Retain this path even if the pointer never moves after navigation.
            root.destination = folder;
            root.openFolder(folder);
        }
    }
}

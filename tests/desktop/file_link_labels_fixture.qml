import QtQuick
import Quickshell
import "LUMEN_ASSETS" as D
import "LUMEN_ASSETS/FileLinks.js" as Links

ShellRoot {
    property int ticks: 0
    property var rows: [
        {name: "ordinary", path: "/fixture/ordinary", directory: false, link: false},
        {name: "resolved", path: "/fixture/resolved", directory: false, link: true, linkStatus: "resolved"},
        {name: "missing", path: "/fixture/missing", directory: false, link: true, linkStatus: "broken"},
        {name: "chain", path: "/fixture/chain", directory: false, link: true, linkStatus: "chain-limit"},
        {name: "denied", path: "/fixture/denied", directory: false, link: true, linkStatus: "permission-denied"}
    ]
    QtObject {
        id: drop
        property var hoverTarget: null
        function registerTarget(value) {}
        function unregisterTarget(value) {}
    }
    FloatingWindow {
        visible: true
        implicitWidth: 900
        implicitHeight: 600
        D.ColumnView { id: columns; width: 450; height: 400; dropCoordinator: drop; columns: [{path: "/fixture", name: "Fixture", loaded: true, rows: rows}] }
        D.FileInfoPanel { id: info; x: 460; width: 350; height: 550; details: ({name: "missing", kind: "Symbolic link", target: "/fixture/old", linkDetail: "The target is missing. Use File Info to inspect the target."}) }
    }
    function labels(item, out) {
        if (item.text !== undefined) out.push(item.text);
        for (const child of item.children || []) labels(child, out);
        return out;
    }
    Timer {
        interval: 40
        repeat: true
        running: true
        onTriggered: {
            try {
                if (++ticks > 75) throw Error("Link label readiness failed");
                const values = labels(columns, []), expected = rows.map(row => Links.markedLabel(row));
                if (!expected.every(value => values.includes(value))) return;
                if (Links.accessible(rows[0]) !== "ordinary" || !Links.accessible(rows[2]).includes("symbolic link") || Links.label(rows[3]) !== "chain (unresolved link)" || !values.includes("↗ resolved") || Links.badge(rows[2]) !== "↗ !" || !Links.markedLabel({name: "long".repeat(64), link: true, linkStatus: "broken"}).startsWith("↗ ! ")) throw Error("Misleading link semantics");
                const details = labels(info, []);
                if (!details.includes("Link Status:") || !details.includes(info.details.linkDetail)) throw Error("Missing recoverable information");
                console.log("LINK_LABELS_QT_PASS");
                Qt.quit();
            } catch (error) {
                console.error("LINK_LABELS_QT_FAIL " + error);
                Qt.quit();
            }
        }
    }
}

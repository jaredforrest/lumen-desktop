import QtQuick
import Quickshell
import "DesktopNavigationModel.js" as Model

ShellRoot {
    Timer {
        running: true
        interval: 10
        onTriggered: {
            try {
                function item(key, x, y, extra) {
                    return Object.assign({key: key, x: x, y: y, width: 108, height: 100}, extra || {})
                }
                function check(actual, expected, message) {
                    if (actual !== expected) throw Error(message + ": " + actual + " != " + expected)
                }
                const items = [item("right-top", 300, 0), item("right-bottom", 300, 200),
                               item("left-top", 0, 0), item("left-bottom", 0, 200)]
                check(Model.neighbour(items, "left-top", "right"), "right-top", "right")
                check(Model.neighbour(items, "right-top", "down"), "right-bottom", "down")
                check(Model.neighbour(items, "right-bottom", "left"), "left-bottom", "left")
                check(Model.neighbour(items, "left-bottom", "up"), "left-top", "up")
                check(Model.neighbour(items, "right-top", "right"), "right-top", "edge")
                check(Model.neighbour(items.concat(item("hidden", 150, 0, {overflow: true})), "left-top", "right"), "right-top", "hidden")
                check(Model.neighbour([item("stack:one", 0, 0, {stack: true}), item("child", 0, 100),
                                       item("stack:two", 200, 0, {stack: true})], "stack:one", "right"), "stack:two", "stack")
                check(Model.neighbour(items, "removed", "down"), "right-top", "replacement")
                check(Model.neighbour([], "removed", "down"), null, "empty")
                check(Model.neighbour(items.slice().reverse(), "left-top", "right"), "right-top", "deterministic")
                console.log("PASS navigation geometry, hidden exclusion, stack identity and replacement")
                Qt.quit()
            } catch (error) {
                console.error("FAIL " + error)
                Qt.exit(1)
            }
        }
    }
}

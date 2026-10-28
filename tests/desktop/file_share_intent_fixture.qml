import QtQuick
import Quickshell
import "FileShareIntent.js" as Share

Timer {
    interval: 100
    running: true
    onTriggered: {
        try {
            function assert(value, message) { if (!value) throw Error(message); }
            const context = {path: "/fixture", tab: "first"};
            const paths = ["/fixture/a $(literal).txt"];
            const rows = [{path: paths[0], directory: false, identity: {device: 4, inode: 8}}];
            const intent = Share.capture(context, paths, rows);
            assert(intent && Share.current(intent, context, paths, rows), "capture/current");
            assert(JSON.stringify(Share.argumentsFor(intent)) === JSON.stringify(["lumen-nearby-sharing", "send", paths[0]]), "literal argv");
            assert(!Share.current(intent, context, ["/fixture/b"], [{path: "/fixture/b", directory: false, identity: {device: 4, inode: 9}}]), "changed path");
            assert(!Share.current(intent, {path: context.path, tab: "second"}, paths, rows), "changed tab");
            assert(!Share.current(intent, {path: "/other", tab: context.tab}, paths, rows), "changed folder");
            assert(!Share.current(intent, context, paths, [{path: paths[0], directory: false, identity: {device: 4, inode: 9}}]), "changed inode");
            assert(!Share.current(intent, context, paths, [{path: paths[0], directory: true, identity: rows[0].identity}]), "changed type");
            assert(!Share.capture(context, paths, [rows[0], rows[0]]), "duplicate rows");
            assert(!Share.capture(context, [paths[0], paths[0]], rows), "duplicate paths");
            assert(!Share.capture(context, [], rows), "empty selection");
            assert(!Share.capture(context, paths, [{path: paths[0], directory: false, identity: null}]), "missing identity");
            assert(!Share.capture({path: "trash:", tab: context.tab}, paths, rows), "trash context");
            assert(!Share.capture(context, Array.from({length: 129}, (_, i) => "/fixture/" + i), []), "oversize selection");
            rows[0].identity.inode = 99;
            assert(intent.selection[0].inode === "8" && !Share.current(intent, context, paths, rows), "snapshot independence");
            const wide = [{path: paths[0], directory: false, identity: {device: "18446744073709551615", inode: "9007199254740993"}}];
            const full = Share.capture(context, paths, wide);
            assert(full && Share.current(full, context, paths, wide) && full.selection[0].inode === "9007199254740993", "wide identity");
            assert(!Share.capture(context, paths, [{path: paths[0], directory: false, identity: {device: "1", inode: "18446744073709551616"}}]), "identity overflow");
            console.log("PASS exact file-share intent");
        } catch (error) {
            console.error("FAIL " + error);
        }
        Qt.quit();
    }
}

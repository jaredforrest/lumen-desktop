import QtQuick
import Quickshell
import "SettingsSearch.js" as Search

Timer {
    interval: 50
    running: true
    onTriggered: {
        try {
            function assert(value, message) { if (!value) throw Error(message); }
            const pages = [...new Set(Search.controls.map(row => row.page))].map(name => ({name: name, terms: ""}));
            assert(new Set(Search.controls.map(row => row.id)).size === Search.controls.length, "duplicate identifier");
            for (const row of Search.controls) {
                assert(row.target && row.label && row.page, "missing control fields");
                assert(Search.results(row.label, pages)[0].control === row.id, "label search " + row.id);
                assert(Search.destination(row.page, row.id, pages).control === row.id, "destination " + row.id);
            }
            assert(Search.destination("Search", "title-bar-double-click-action", pages) === null, "wrong page");
            assert(Search.destination("Unknown", "", pages) === null, "unknown page");
            assert(Search.destination("Search", "bogus", pages) === null, "stale identifier");
            assert(Search.results("   ", pages).length === 0, "empty query");
            assert(Search.results("double-click title", pages)[0].control === "title-bar-double-click-action", "word order");
            assert(Search.results("No setting with this visible label", pages).length === 0, "missing match");
            console.log("PASS typed settings destinations " + Search.controls.length);
        } catch (error) {
            console.error("FAIL " + error);
        }
        Qt.quit();
    }
}

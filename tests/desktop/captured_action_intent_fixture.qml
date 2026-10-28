import QtQuick
import Quickshell
import "LUMEN_ASSETS" as D

ShellRoot {
    id: root
    property var activated: []
    D.CapturedActionButton {
        id: button
        selection: ({capability: "first", session: "provider-one"})
        onActivatedIntent: value => root.activated = root.activated.concat([value])
    }
    function check(value, message) { if (!value) throw Error(message); }
    Timer { interval: 1000; running: true; onTriggered: Qt.quit() }
    Component.onCompleted: {
        button.beginIntent(); button.selection = {capability: "second", session: "provider-one"}; button.clicked(); check(activated.length === 0, "Held pointer retargeted new resource");
        button.beginIntent(); button.selection = {capability: "second", session: "provider-two"}; button.clicked(); check(activated.length === 0, "Provider replacement retained old intent");
        button.beginIntent(); button.cancelIntent(); button.selection = null; button.clicked(); check(activated.length === 0, "Canceled intent activated");
        button.selection = {capability: "second", session: "provider-two"}; button.clicked(); check(activated.length === 1 && activated[0].capability === "second", "Keyboard activation failed exact current capture");
        button.beginIntent(); button.clicked(); check(activated.length === 2, "Exact pointer activation missing");
        button.selection = {capability: "second", session: "provider-two", choice: "chooser", application: "first.desktop"}; button.beginIntent(); button.selection = {capability: "second", session: "provider-two", choice: "chooser", application: "second.desktop"}; button.clicked(); check(activated.length === 2, "Application delegate was retargeted during held press");
        console.log("PASS captured pointer, cancellation, keyboard and provider identities"); Qt.quit();
    }
}

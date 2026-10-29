import QtQuick
import Quickshell
import "."

Timer {
    interval: 300
    running: true
    repeat: false
    property var available: DesktopEntries.applications.values

    function equal(actual, expected) {
        if (JSON.stringify(actual) !== JSON.stringify(expected))
            throw new Error(JSON.stringify(actual) + " != " + JSON.stringify(expected));
    }

    onTriggered: {
        try {
            const entry = {command:["app", "$HOME", "literal value"], actions:[
                {id:"new-private-window",command:["private"]},
                {id:"new-window",command:["env", "SPECIAL=1", "app", "--new-instance"]}],
                runInTerminal:false};
            equal(AppLaunch.commandFor(entry,false), entry.command);
            equal(AppLaunch.commandFor(entry,true), entry.actions[1].command);
            entry.runInTerminal=true;
            equal(AppLaunch.commandFor(entry,true), ["foot","-e"].concat(entry.actions[1].command));
            entry.actions=[];
            equal(AppLaunch.commandFor(entry,true), ["foot","-e"].concat(entry.command));
            entry.runInTerminal=false;
            entry.actions=[{id:"NewDocument",command:["writer","--new"]}];
            equal(AppLaunch.commandFor(entry,true), ["writer","--new"]);
            const real = DesktopEntries.byId("lumen-launch-test");
            if (!real) throw new Error("fixture desktop entry missing");
            equal(AppLaunch.commandFor(real,true), ["env","MULTI=1","app","--new-instance"]);
            console.log("PASS launch actions, literal arguments, terminal wrapper, fallback and Quickshell desktop parser");
        } catch (error) { console.error("FAIL " + error); }
        Qt.quit();
    }
}

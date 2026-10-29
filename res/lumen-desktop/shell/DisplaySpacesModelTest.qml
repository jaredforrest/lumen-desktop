import QtQuick
import Quickshell
import "DisplaySpacesModel.js" as Model

ShellRoot {
    function check(value, message) { if (!value) throw Error(message); }
    Timer {
        interval: 20
        running: true
        onTriggered: {
            try {
                const identity = {kind: "connector", name: "A", make: null, model: null, serial: null};
                const current = {protocolVersion: 1, nativeSupported: true,
                    session: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", generation: "18446744073709551614",
                    available: true, applied: true, separateDisplays: false, independent: false,
                    primary: "A", primaryDisplay: null,
                    outputs: [{name: "A", label: "Display", identity: identity}], error: ""};
                check(Model.valid(current, null), "valid authority refused");
                const changed = [
                    {primaryDisplay: Object.assign({}, identity, {name: "B"}), outputs: current.outputs.concat([
                        {name: "B", label: "Other", identity: Object.assign({}, identity, {name: "B"})}])},
                    {independent: true}, {separateDisplays: true, independent: false},
                    {generation: 1}, {generation: "01"}, {generation: "18446744073709551616"},
                    {session: "unknown"}, {available: 1}, {applied: true, available: false},
                    {primary: "gone"}, {outputs: current.outputs.concat([current.outputs[0]])},
                    {outputs: [{name: "A", label: "X", identity: {kind: "connector", name: "A"}}]}
                ];
                for (const change of changed) check(!Model.valid(Object.assign({}, current, change), null), "invalid authority accepted: " + JSON.stringify(change));
                check(!Model.valid(Object.assign({}, current, {generation: "18446744073709551613"}), current), "stale generation accepted");
                check(Model.valid(Object.assign({}, current, {generation: "18446744073709551615"}), current), "next generation refused");
                check(Model.valid(Object.assign({}, current, {session: "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb", generation: "0"}), current), "new session refused");
                const legacy = Object.assign({}, current, {nativeSupported: false, session: null, generation: null,
                    available: false, applied: false, separateDisplays: false, independent: true,
                    primary: "", error: "Upgrade required"});
                check(Model.valid(legacy, current), "legacy fallback refused");
                let route = Model.routing(null, legacy);
                check(Model.visible(route, "A", [{name: "A"}]), "legacy A hidden");
                check(Model.visible(route, "B", [{name: "B"}]), "legacy B hidden");
                route = Model.routing(route, current);
                check(!Model.visible(route, "B", [{name: "A"}, {name: "B"}]), "secondary Dock shown in coupled mode");
                check(Model.destination(route, "B", [{name: "A"}, {name: "B"}]) === "A", "Dock did not route to main");
                route = Model.routing(route, {independent: false, applied: false});
                check(Model.destination(route, "B", [{name: "B", x: 200}, {name: "C", x: 0}]) === "C", "disconnected main fallback failed");
                check(Model.visible(route, "C", [{name: "B", x: 200}, {name: "C", x: 0}]), "fallback Dock hidden");
                check(!Model.visible(route, "B", [{name: "B", x: 200}, {name: "C", x: 0}]), "secondary fallback Dock visible");
                console.log("DISPLAY_MODEL_PASS");
            } catch (error) {
                console.error("DISPLAY_MODEL_FAIL " + error);
            }
            Qt.quit();
        }
    }
}

import QtQuick
import Quickshell
import "NetworkState.js" as NetworkState

ShellRoot {
    function check(value, message) { if (!value) throw Error(message); }
    Timer {
        interval: 0; running: true; repeat: false
        onTriggered: {
            try {
                for (const value of [undefined, null, false, 1, "unavailable", [], {}]) {
                    const row = NetworkState.internet(value);
                    check(row.status === "unknown" && typeof row.message === "string", "Absent status");
                    check(row.checks.length === 0 && !row.routes["4"].available && !row.vpn.selected_route, "Absent details");
                }
                const row = NetworkState.internet({status:"online",message:"Working",routes:{"4":{available:true,interface:"private0"},"6":null},dns:{status:"ok"},vpn:{state:"connected",selected_route:true,locked_down:false},checks:[{ok:true,endpoint:"not forwarded"},null,{ok:false},{ok:1}],secret:"not forwarded"});
                check(row.status === "online" && row.message === "Working" && row.routes["4"].interface === "private0", "Populated record");
                check(row.checks.length === 2 && row.checks[0].ok === true, "Check filtering");
                check(!("endpoint" in row.checks[0]) && !("secret" in row), "Unrelated payload forwarded");
                const bad = NetworkState.internet({status:"nonsense",message:{},checks:"not an array",routes:{"4":{available:"yes",interface:5}},vpn:{selected_route:"yes",locked_down:1}});
                check(bad.status === "unknown" && typeof bad.message === "string" && bad.checks.length === 0, "Malformed record");
                check(!bad.routes["4"].available && !bad.vpn.locked_down, "Malformed booleans");
                check(NetworkState.internet({checks:Array.from({length:50},()=>({ok:true}))}).checks.length === 32, "Check limit");
                check(NetworkState.internet({message:"x".repeat(513)}).message === "Internet monitoring is unavailable.", "Message limit");
                for (const status of ["offline", "degraded", "portal"]) check(NetworkState.internet({status}).status === status, "Status option");
                console.log("NETWORK_HEALTH_PRESENTATION_PASS"); Qt.quit();
            } catch (error) { console.error("NETWORK_HEALTH_PRESENTATION_FAIL " + error); Qt.quit(); }
        }
    }
}

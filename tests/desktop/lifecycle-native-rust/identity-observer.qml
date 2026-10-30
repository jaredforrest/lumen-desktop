import QtQuick
import Quickshell
import Quickshell.Io
import Quickshell.Wayland
ShellRoot {
    IpcHandler {
        target:"identityAudit"
        function status():string {
            return JSON.stringify({epoch:"__EPOCH__",windows:ToplevelManager.toplevels.values.map(window=>({
                identifier:typeof window.foreignToplevelIdentifier==="string"?window.foreignToplevelIdentifier:"",
                appId:window.appId,title:window.title
            }))});
        }
    }
}

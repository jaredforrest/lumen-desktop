import QtQuick
import Quickshell
import Quickshell.Io
import "ShortcutHints.js" as ShortcutHints
QtObject {
    id:root
    property bool available:false
    property string error:""
    property var bindings:[]
    property var actions:[]
    function clear(message){available=false;bindings=[];actions=[];error=message;}
    function receive(text){try{const parsed=ShortcutHints.read(JSON.parse(text));bindings=parsed.bindings;actions=parsed.hints;available=true;error="";}catch(error){clear("Configured shortcuts are unavailable");}}
    function hint(action,appId){return available?ShortcutHints.hint(actions,action,appId || ""):"";}
    function menuHint(action){return hint(ShortcutHints.menuAction(action));}
    property FileView source:FileView {
        path:(Quickshell.env("XDG_CONFIG_HOME") || Quickshell.env("HOME")+"/.config")+"/lumen-desktop/shortcuts.json"
        printErrors:false;watchChanges:true
        onFileChanged:reload()
        onLoaded:root.receive(text())
        onLoadFailed:root.clear("Configured shortcuts are unavailable")
    }
}

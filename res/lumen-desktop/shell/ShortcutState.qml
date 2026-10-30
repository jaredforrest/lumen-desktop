import QtQuick
import Quickshell
import Quickshell.Io
import "ShortcutHints.js" as ShortcutHints
QtObject {
    id:root
    readonly property string layoutGeneration:ShortcutLayoutState.generation
    property bool available:false
    property string error:""
    property var bindings:[]
    property var actions:[]
    function clear(message){available=false;bindings=[];actions=[];error=message;}
    function receive(text){try{const parsed=ShortcutHints.read(JSON.parse(text));bindings=parsed.bindings;actions=parsed.hints;available=true;error="";}catch(error){clear("Configured shortcuts are unavailable");}}
    function hint(action,appId){
        if(!available)return "";
        const local=appId?actions.find(group=>group.action===action && group.scope==="application" && group.appId===appId):null;
        const group=local || actions.find(group=>group.action===action && group.scope==="global");
        if(!group)return "";
        if(ShortcutLayoutState.enabled)return ShortcutLayoutState.available?ShortcutLayoutState.hint(group.bindings[0].id):"";
        return ShortcutHints.hint(actions,action,appId || "");
    }
    function menuHint(action){return hint(ShortcutHints.menuAction(action));}
    property FileView source:FileView {
        path:(Quickshell.env("XDG_CONFIG_HOME") || Quickshell.env("HOME")+"/.config")+"/lumen-desktop/shortcuts.json"
        printErrors:false;watchChanges:true
        onFileChanged:reload()
        onLoaded:root.receive(text())
        onLoadFailed:root.clear("Configured shortcuts are unavailable")
    }
}

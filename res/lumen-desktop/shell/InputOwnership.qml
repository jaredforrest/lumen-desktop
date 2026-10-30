pragma Singleton
import QtQuick
QtObject {
    // The focused editor, not the enclosing popup, owns a live preedit.
    function composing(window) {
        let item=window?window.activeFocusItem:null;
        while(item){if(item.inputMethodComposing===true)return true;item=item.parent;}
        return false;
    }
    function textOwns(event,window) {
        return composing(window) || !!(event.modifiers & Qt.GroupSwitchModifier);
    }
}

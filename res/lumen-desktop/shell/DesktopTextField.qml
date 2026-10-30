import QtQuick
import QtQuick.Controls
TextField {
    id:control
    function compositionReturn(event) {
        if(!inputMethodComposing && !(event.modifiers & Qt.GroupSwitchModifier)){event.accepted=false;return;}
        // Replace Qt's commit-plus-accepted path with a commit request only.
        // Candidate confirmation must not submit the containing form as well.
        event.accepted=true;
        if(inputMethodComposing)Qt.inputMethod.commit();
    }
    Keys.onReturnPressed:event=>compositionReturn(event)
    Keys.onEnterPressed:event=>compositionReturn(event)
    Keys.onEscapePressed:event=>{
        if(inputMethodComposing){event.accepted=true;Qt.inputMethod.reset();}
        else event.accepted=false;
    }
}

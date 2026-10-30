import QtQuick
import QtQuick.Controls
import "ShortcutHints.js" as ShortcutHints
Column {
    id:root
    objectName:"settings-configured-shortcuts"
    property bool active:false
    property var service:owned
    property string selected:""
    property string capturedGeneration:""
    property bool dirty:false
    readonly property var row:service.bindings.find(item=>item.id===selected) || service.bindings[0] || null
    function synchronize(){selector.currentIndex=row?service.bindings.findIndex(item=>item.id===row.id):-1;if(dirty)return;chord.text=row && row.chord?row.chord:"";capturedGeneration=service.generation;}
    function submit(action){if(!row || !row.editable)return;service.change(action,row.id,chord.text,capturedGeneration);dirty=false;}
    activeFocusOnTab:true;Accessible.role:Accessible.Grouping;Accessible.name:"Configured Shortcuts"
    spacing:8
    ShortcutEditorState {id:owned;active:root.active && root.service===owned}
    Connections {target:root.service;function onGenerationChanged(){root.synchronize();}function onBindingsChanged(){Qt.callLater(root.synchronize);}}
    PanelLabel {text:"Configured Shortcuts";font.weight:Font.DemiBold}
    PanelLabel {width:parent.width;wrapMode:Text.Wrap;color:Theme.colors.muted;text:root.service.editable?"Changes rebuild the authoritative configuration. Apply succeeds only when the effective map confirms the change.":"Read-only configuration. Shortcut editing requires an explicitly wired consumer configuration."}
    PanelLabel {width:parent.width;wrapMode:Text.Wrap;color:Theme.colors.muted;text:root.service.boundary}
    PanelLabel {width:parent.width;wrapMode:Text.Wrap;color:Theme.colors.muted;visible:root.service.configuredEditable && !root.service.runtimeConfirmed;text:root.service.runtimeDetail}
    StatusMessage {width:parent.width;text:root.service.error;severity:"error"}
    StatusMessage {width:parent.width;text:root.service.message;severity:"success"}
    PanelLabel {width:parent.width;visible:root.service.busy;wrapMode:Text.Wrap;text:root.service.request.action==="status"?"Reading effective shortcuts…":"Building and applying the requested shortcut. The previous map remains active until activation completes."}
    DesktopComboBox {id:selector;objectName:"shortcut-editor-action";width:parent.width;Accessible.name:"Shortcut action";model:root.service.bindings.map(item=>ShortcutHints.actionName(item.action)+" · "+ShortcutHints.readableChord(item.defaultChord)+(item.enabled?"":" · disabled"));currentIndex:-1;enabled:!root.service.busy && count>0;onActivated:index=>{root.selected=root.service.bindings[index].id;root.dirty=false;root.synchronize();}}
    PanelLabel {objectName:"shortcut-layout-summary";width:parent.width;wrapMode:Text.Wrap;color:Theme.colors.muted;text:ShortcutLayoutState.summary}
    PanelLabel {objectName:"shortcut-layout-observed-bindings";width:parent.width;wrapMode:Text.Wrap;visible:ShortcutLayoutState.available;text:ShortcutLayoutState.bindingSummary}
    PanelLabel {objectName:"shortcut-layout-keycaps";width:parent.width;wrapMode:Text.Wrap;visible:root.row!==null;text:root.row?"Current keycaps: "+(ShortcutLayoutState.keycaps(root.row.id) || "unavailable")+ (ShortcutLayoutState.collision(root.row.id)?" · Physical shortcut collision detected":""):""}
    PanelLabel {objectName:"shortcut-layout-native-bindings";width:parent.width;wrapMode:Text.Wrap;visible:ShortcutLayoutState.available && text.length>0;text:"Native shortcut diagnostics (up to 16 per seat):\n"+ShortcutLayoutState.nativeSummary}
    PanelLabel {objectName:"shortcut-layout-boundary";width:parent.width;wrapMode:Text.Wrap;color:Theme.colors.muted;visible:ShortcutLayoutState.available;text:ShortcutLayoutState.diagnostics.join(". ")}
    DesktopTextField {
        id:chord;objectName:"shortcut-editor-chord";width:parent.width;Accessible.name:"Shortcut symbol chord"
        placeholderText:"For example Super+Control+F12";color:Theme.colors.text;font.family:Theme.font;font.pixelSize:Theme.textSize(12);selectByMouse:true
        background:Rectangle {radius:6;color:Theme.colors.field;border.color:chord.activeFocus?Theme.colors.accent:Theme.hairline}
        enabled:root.service.editable && root.row && root.row.editable && !root.service.busy;onTextEdited:root.dirty=true
    }
    Flow {width:parent.width;spacing:6
        DialogButton {objectName:"shortcut-editor-apply";text:"Apply";enabled:chord.enabled && chord.text.length>0;onClicked:root.submit("set")}
        DialogButton {objectName:"shortcut-editor-disable";text:"Disable";enabled:chord.enabled && root.row.enabled;onClicked:root.submit("disable")}
        DialogButton {objectName:"shortcut-editor-restore";text:"Restore default";enabled:chord.enabled;onClicked:root.submit("restore")}
        DialogButton {objectName:"shortcut-editor-discard";text:"Discard edit";enabled:root.dirty && !root.service.busy;onClicked:{root.dirty=false;root.synchronize();}}
    }
}

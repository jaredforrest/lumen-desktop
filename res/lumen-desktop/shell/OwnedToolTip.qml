import QtQuick
import QtQuick.Controls
import QtQuick.Window
ToolTip {
    id:root
    objectName:"owned-tooltip"
    property Item target:null
    property bool hover:!!target && target.hovered===true
    property bool pressed:!!target && target.pressed===true
    property bool available:!!target && target.enabled && target.visible
    property bool suppressed:false
    readonly property bool editorComposing:InputOwnership.composing(root.target?root.target.Window.window:null)
    parent:target
    property TooltipState lifecycle:TooltipState {eligible:root.available && !root.suppressed && !root.pressed && (root.hover || !!root.target && root.target.activeFocus);identity:root.text}
    visible:lifecycle.shown
    delay:0;timeout:-1;focus:false;padding:7
    closePolicy:editorComposing?Popup.CloseOnPressOutsideParent:Popup.CloseOnEscape | Popup.CloseOnPressOutsideParent
    margins:8
    width:Math.min(360,Math.max(1,(target && target.Window.window?target.Window.window.width:360)-16),label.implicitWidth+2*padding)
    height:Math.min(Math.max(1,(target && target.Window.window?target.Window.window.height:480)-16),label.implicitHeight+2*padding)
    x:target?Math.max(8-target.mapToItem(null,0,0).x,Math.min((target.width-width)/2,(target.Window.window?target.Window.window.width:360)-8-target.mapToItem(null,0,0).x-width)):0
    y:target?target.height+8:0
    onHoverChanged:if(!hover)lifecycle.dismiss();else if(available && !suppressed)lifecycle.restart()
    onVisibleChanged:if(!visible && lifecycle.shown)lifecycle.dismiss()
    onTargetChanged:lifecycle.restart()
    contentItem:Text {id:label;text:root.text;textFormat:Text.PlainText;wrapMode:Text.Wrap;clip:true;color:Theme.colors.text;font.family:Theme.font;font.pixelSize:Theme.textRole("metadata")}
    background:Rectangle {radius:6;color:Theme.colors.background;border.color:Theme.outline;border.width:1}
    Shortcut {sequence:"F1";context:Qt.WindowShortcut;enabled:root.available && !!root.target && root.target.activeFocus && !root.suppressed && !root.editorComposing;onActivated:lifecycle.help()}
    Shortcut {sequence:"Escape";context:Qt.WindowShortcut;enabled:root.visible && !root.editorComposing;onActivated:lifecycle.dismiss()}
    Connections {target:root.target ? root.target.Window.window : null;function onActiveChanged(){if(!target.active)root.lifecycle.dismiss();}}
}

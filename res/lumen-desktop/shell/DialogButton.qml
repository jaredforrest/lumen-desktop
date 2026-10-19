import QtQuick
import QtQuick.Controls
AbstractButton {
    id: control
    property bool primary: false
    property bool destructive: false
    property bool defaultAction: primary && !destructive
    readonly property color actionColor: destructive ? Theme.errorText : Theme.colors.accent
    Accessible.name: text
    Keys.onReturnPressed: if (enabled) clicked()
    Keys.onEnterPressed: if (enabled) clicked()
    Shortcut {sequence:"Return";context:Qt.WindowShortcut;enabled:control.defaultAction && control.enabled && control.visible && !control.activeFocus;onActivated:control.clicked()}
    implicitHeight: Math.max(28,label.implicitHeight+12)
    implicitWidth: Math.max(80, label.implicitWidth + 28)
    opacity: enabled ? 1 : 0.45
    contentItem: PanelLabel {
        id: label
        text: control.text
        wrapMode:Text.Wrap
        elide:Text.ElideNone
        horizontalAlignment: Text.AlignHCenter
        verticalAlignment: Text.AlignVCenter
        color: control.primary ? (control.destructive ? Theme.destructiveText : Theme.colors.selectedText) : control.destructive ? control.actionColor : Theme.colors.text
    }
    background: Rectangle {
        radius: 6
        color: control.primary ? (control.destructive ? Theme.destructiveFill : control.actionColor) : control.down || control.hovered ? Theme.colors.hover : Theme.colors.surface
        border.width: control.activeFocus || control.defaultAction ? 2 : 1
        border.color: control.activeFocus || control.defaultAction ? control.actionColor : control.primary ? control.actionColor : Theme.outline
        Rectangle { anchors.fill: parent; radius: parent.radius; color: "black"; opacity: control.primary && control.down ? 0.15 : 0 }
    }
}

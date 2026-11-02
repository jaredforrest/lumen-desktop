import QtQuick
import QtQuick.Controls
import QtQuick.Window
import Quickshell
Row {
    id: root
    visible: !Theme.nativeWindowFrame
    property bool windowActive: true
    property string windowName: "Window"
    property bool canMinimize: true
    property bool canFullscreen: true
    // All desktop-owned windows use the same inset and traffic-light geometry.
    x: 10
    y: 14
    readonly property bool controlsHovered: [0,1,2].some(i=>{const b=lights.itemAt(i);return b && b.hovered;})
    signal triggered(int action)
    function activate(index, modifiers) {
        if (Theme.nativeWindowActions && root.Window.window) {
            const action = index === 2 && (modifiers & Qt.AltModifier) ? "fill" : ["close","minimize","fullscreen"][index];
            Quickshell.execDetached(["lumen-focus-window", "org.quickshell", root.Window.window.title, "--action", action]);
        } else root.triggered(index);
    }
    spacing: 0
    Repeater {
        id:lights
        model: ["#ff5f57", "#febc2e", "#28c840"]
        AbstractButton {
            id: button
            required property int index
            required property string modelData
            width: 20; height: 20
            enabled: index === 0 || (index === 1 ? root.canMinimize : root.canFullscreen)
            Accessible.name: ["Close " + root.windowName, "Minimize " + root.windowName, "Enter full screen"][index]
            background: Rectangle {
                anchors.centerIn: parent
                width: 12; height: 12
                radius: 6
                color: button.enabled && root.windowActive ? button.modelData : Theme.dark ? "#555559" : "#ceced2"
                border.width: 1
                border.color: button.activeFocus ? Theme.colors.accent : Qt.darker(color, 1.12)
                FocusOutline {focused:button.activeFocus;inset:-3;radius:9}
            }
            contentItem: Item {
                SystemGlyph {
                    visible: button.enabled && (root.controlsHovered || button.activeFocus)
                    anchors.centerIn: parent
                    name: button.index === 0 ? "close" : button.index === 1 ? "minus" : "plus"
                    width: 8; height: 8; ink: "#333333"
                }
            }
            onClicked: root.activate(index, 0)
            MouseArea {
                anchors.fill:parent
                enabled:button.enabled
                onClicked: mouse => root.activate(button.index, mouse.modifiers)
            }
        }
    }
}

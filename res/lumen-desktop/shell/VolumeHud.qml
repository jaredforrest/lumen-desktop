import QtQuick
import Quickshell
import Quickshell.Wayland
PanelWindow {
    id: root
    required property var shell
    required property string outputName
    visible: shell.hudOutput === outputName && shell.hudVisible
    anchors.bottom: true
    margins.bottom: 120
    implicitWidth: 190; implicitHeight: 146
    exclusiveZone: 0
    color: "transparent"
    mask: Region {}
    WlrLayershell.layer: WlrLayer.Overlay
    WlrLayershell.keyboardFocus: WlrKeyboardFocus.None
    WlrLayershell.namespace: "lumen-volume-hud"
    PanelSurface {
        id:hudSurface
        opacity:root.visible?1:0
        Behavior on opacity {NumberAnimation {duration:Theme.preferences.reduceMotion?0:100}}
        anchors.fill:parent;anchors.margins:8;radius:22
        SystemGlyph {anchors.horizontalCenter:parent.horizontalCenter;y:23;width:45;height:45;name:root.shell.audioState.muted?"muted":"speaker"}
        PanelLabel {anchors.horizontalCenter:parent.horizontalCenter;y:78;text:!root.shell.audioState.available?"No Output":root.shell.audioState.muted?"Muted":Math.round(root.shell.audioState.level*100)+"%"}
        Rectangle {x:20;y:105;width:parent.width-40;height:5;radius:2.5;color:Theme.colors.field
            Rectangle {height:parent.height;width:root.shell.audioState.muted?0:parent.width*root.shell.audioState.level;radius:2.5;color:Theme.colors.text}
        }
    }
}

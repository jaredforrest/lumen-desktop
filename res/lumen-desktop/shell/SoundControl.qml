import QtQuick
import QtQuick.Controls
import Quickshell
Column {
    id: root
    required property var shell
    property bool compact: false
    spacing: compact ? 6 : 10
    property bool available: /Volume:\s*[\d.]+/.test(shell.volume)
    property real volume: { const m=shell.volume.match(/Volume:\s*([\d.]+)/); return m ? Math.min(1,Number(m[1])) : 0; }
    Row {
        width:parent.width
        PanelLabel { width:parent.width-50; text:"Sound"; font.pixelSize:Theme.textSize(root.compact?12:13); font.weight:Font.DemiBold }
        PanelLabel { visible:!root.compact; width:50; horizontalAlignment:Text.AlignRight; text:!root.available?"—":root.shell.volume.includes("MUTED") ? "Muted" : Math.round(root.volume*100)+"%"; color:Theme.colors.muted }
    }
    SystemSlider {
        width:parent.width;compact:root.compact;from:0;to:1;value:root.volume;enabled:root.available
        leadingGlyph:root.shell.volume.includes("MUTED")?"muted":"speaker-low";trailingGlyph:"speaker"
        onPressedChanged:root.shell.editVolume(pressed)
        onMoved:root.shell.setVolume(value)
        Accessible.name:"Output volume"
    }
}

import QtQuick
import QtQuick.Effects
Rectangle {
    radius: 24
    color: Theme.popoverMaterial
    border.color: Theme.outline
    border.width: 1
    MouseArea { anchors.fill: parent }
    layer.enabled: GraphicsInfo.api !== GraphicsInfo.Software
    layer.effect: MultiEffect {
        shadowEnabled: true
        shadowBlur: 0.65
        shadowVerticalOffset: 5
        shadowColor: Theme.dark ? "#60000000" : "#30000000"
    }
}

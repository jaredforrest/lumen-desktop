import QtQuick
import QtQuick.Controls
import "BatteryModel.js" as Model
AbstractButton {
    id:root
    property var power:Model.unavailable()
    readonly property var validated:Model.normalize(power)
    hoverEnabled:true;activeFocusOnTab:true
    implicitWidth:70;implicitHeight:24
    visible:validated.available && validated.present
    Accessible.name:"Battery: "+Model.description(validated)
    Accessible.description:Model.estimate(validated)
    OwnedToolTip {target:root;text:Model.description(validated)+(Model.estimate(validated)?"\n"+Model.estimate(validated):"")}
    background:Rectangle {radius:6;color:root.hovered || root.activeFocus?Theme.colors.hover:"transparent";border.width:root.activeFocus?1:0;border.color:Theme.colors.accent}
    contentItem:Row {spacing:4
        BatteryGlyph {anchors.verticalCenter:parent.verticalCenter;percentage:root.validated.percentage || 0;charging:root.validated.state==="charging"}
        PanelLabel {anchors.verticalCenter:parent.verticalCenter;text:root.validated.present?Math.round(root.validated.percentage)+"%":"";font.pixelSize:12}
    }
}

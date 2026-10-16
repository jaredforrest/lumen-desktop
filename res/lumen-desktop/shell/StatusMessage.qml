import QtQuick
PanelLabel {
    property string severity:"info"
    visible: text.length>0
    wrapMode: Text.WordWrap
    elide: Text.ElideNone
    color: severity==="error" ? (Theme.dark?"#ff6961":"#b42318") : severity==="success" ? (Theme.dark?"#55d788":"#21743e") : Theme.colors.muted
    font.pixelSize:12
    Accessible.role: Accessible.StaticText
    Accessible.name:(severity==="error"?"Error: ":severity==="success"?"Success: ":"")+text
    Accessible.description: severity
}

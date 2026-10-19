import QtQuick
PanelLabel {
    property string severity:"info"
    visible: text.length>0
    wrapMode: Text.WordWrap
    elide: Text.ElideNone
    color: severity==="error" ? Theme.errorText : severity==="success" ? Theme.successText : severity==="warning" ? Theme.warningText : Theme.colors.muted
    font.pixelSize:Theme.textSize(12)
    Accessible.role: Accessible.StaticText
    Accessible.name:(severity==="error"?"Error: ":severity==="success"?"Success: ":severity==="warning"?"Warning: ":"")+text
    Accessible.description: severity
}

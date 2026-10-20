import QtQuick
Text {
    textFormat: Text.PlainText
    color: Theme.colors.text
    font.family: Theme.font
    property string textRole: "body"
    font.pixelSize: Theme.textRole(textRole)
    elide: Text.ElideRight
}

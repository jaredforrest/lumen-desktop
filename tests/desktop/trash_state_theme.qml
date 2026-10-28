pragma Singleton
import QtQuick
QtObject {
    property string font: "sans-serif"
    property var colors: ({text: "#111111", surface: "#eeeeee", border: "#777777", accent: "#0066bb", selectedText: "#ffffff", hover: "#dddddd"})
    property color outline: "#777777"
    property color errorText: "#aa0000"
    property color destructiveText: "white"
    property color destructiveFill: "#aa0000"
    function textSize(value) { return value; }
}

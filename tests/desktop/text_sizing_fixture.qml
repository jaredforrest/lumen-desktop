import QtQuick
import QtQuick.Window
import Quickshell
import "LUMEN_ASSETS" as D

ShellRoot {
    D.DesktopComboBox { id: baseline; width: 400; model: ["Standard"] }
    Window {
        visible: true
        width: 250
        height: 800
        Column {
            width: 190
            spacing: 10
            D.SettingsToggle { id: toggle; width: 190; text: "Vergrößerte Benutzeroberfläche vollständig anzeigen"; detail: "Lange übersetzte Erläuterungen bleiben vollständig lesbar und überdecken keine Schalter." }
            D.ControlRow { id: row; width: 190; text: "Zusätzliche Datenschutzeinstellungen öffnen"; detail: "Separate Informationen für diese Anwendung und die ausgewählte Ressource" }
            D.DialogButton { id: button; width: 190; text: "Ausgewählte Einstellungen jetzt bestätigen" }
            D.DesktopComboBox { id: combo; width: 190; model: ["Erweiterte Benutzeroberflächeneinstellungen vollständig anzeigen"] }
        }
    }
    function labels(item, result) {
        if (item.text !== undefined && item.font !== undefined) result.push(item);
        for (const child of item.children || []) labels(child, result);
    }
    Timer {
        interval: 150
        running: true
        onTriggered: {
            try {
                if (D.Theme.textScale !== 1.5) throw Error("Preference not loaded");
                for (const control of [toggle, row, button, combo]) {
                    const text = [];
                    labels(control.contentItem, text);
                    if (!text.length) throw Error("No actual text");
                    for (const label of text) {
                        if (label.visible && label.text) {
                            if (label.truncated || label.height + 0.5 < label.implicitHeight) throw Error("Clipped long label " + label.text);
                            if (label.font.pixelSize < 15) throw Error("Text not enlarged");
                        }
                    }
                    if (control.height < control.contentItem.implicitHeight) throw Error("Content outside control");
                }
                if (toggle.height <= 58 || row.height <= 46 || button.height <= 28 || combo.height <= 30) throw Error("Controls failed to grow");
                D.Theme.preferences = {textScale: 1};
                if (D.Theme.textSize(13) !== 13) throw Error("Restore default text size");
                if (baseline.font.pixelSize !== 13 || baseline.contentItem.font.pixelSize !== 13 || baseline.height !== 30) throw Error("Default combo text/height changed");
                console.log("PASS enlarged control labels");
            } catch (error) {
                console.error("FAIL " + error);
            }
            Qt.quit();
        }
    }
}

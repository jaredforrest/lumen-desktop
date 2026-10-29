import QtQuick
import QtQuick.Window

Window {
    id: root
    property var config: CONFIG_JSON
    property string outputPath: OUTPUT_JSON
    visible: true
    width: 1400 * config.scale
    height: (60 + config.cases.length * 76) * config.scale
    color: config.background

    Rectangle {
        anchors.fill: parent
        color: root.config.background
    Repeater {
        id: rows
        model: root.config.cases
        delegate: Item {
            id: row
            required property int index
            required property var modelData
            property alias sample: sample
            x: 0
            y: index * 76 * root.config.scale
            width: root.width
            height: 76 * root.config.scale

            Text {
                x: 24 * root.config.scale
                y: 18 * root.config.scale
                text: row.modelData.name
                color: root.config.muted
                font.family: root.config.family
                font.pixelSize: 12 * root.config.scale
                renderType: Text.NativeRendering
            }
            Text {
                id: sample
                x: 24 * root.config.scale
                y: 37 * root.config.scale
                width: root.width - 48 * root.config.scale
                text: row.modelData.text
                color: root.config.foreground
                font.family: root.config.family
                font.pixelSize: (row.modelData.size || 22) * root.config.scale
                font.features: row.modelData.tabular ? ({"tnum": 1}) : ({})
                renderType: Text.NativeRendering
                wrapMode: Text.NoWrap
            }
        }
    }
    }

    TextMetrics { id: digitMetrics }
    Timer {
        interval: 250
        running: true
        onTriggered: {
            try {
                const rendered = [];
                for (let i = 0; i < root.config.cases.length; i++) {
                    const row = rows.itemAt(i);
                    if (!row || row.sample.contentWidth <= 0 || row.sample.contentHeight <= 0)
                        throw Error("Text metrics unavailable for row " + i);
                    const widths = [];
                    if (row.modelData.tabular) {
                        digitMetrics.font = row.sample.font;
                        for (let digit = 0; digit < 10; digit++) {
                            digitMetrics.text = String(digit);
                            widths.push(digitMetrics.advanceWidth / root.config.scale);
                        }
                    }
                    rendered.push({name: row.modelData.name, text: row.modelData.text,
                        width: row.sample.contentWidth / root.config.scale,
                        height: row.sample.contentHeight / root.config.scale,
                        digitWidths: widths, resolvedFamily: row.sample.fontInfo.family});
                }
                if (!root.contentItem.grabToImage(function(frame) {
                    if (!frame.saveToFile(root.outputPath)) {
                        console.error("LUMEN_TYPOGRAPHY_FAILURE image save failed");
                    } else {
                        console.log("LUMEN_TYPOGRAPHY_RESULT " + JSON.stringify({
                            qtVersion: QT_VERSION, requestedFamily: root.config.family,
                            scale: root.config.scale, cases: rendered
                        }));
                    }
                    Qt.quit();
                })) throw Error("Image capture request failed");
            } catch (error) {
                console.error("LUMEN_TYPOGRAPHY_FAILURE " + error);
                Qt.quit();
            }
        }
    }
}

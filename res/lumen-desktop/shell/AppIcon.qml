import QtQuick

Image {
    id: root
    property url iconSource
    property date calendarDate: new Date()
    readonly property string variantName: {
        const match=String(iconSource).match(/\/reference-icons\/(calendar|notes|reminders)\.(png|svg)$/);
        return match ? match[1] : "";
    }
    readonly property bool usingDarkVariant: variantName !== "" && (Theme.iconAppearance === "dark" || (Theme.iconAppearance === "auto" && Theme.dark))
    readonly property url desiredSource: usingDarkVariant ? Qt.resolvedUrl("reference-icons/dark/"+variantName+".svg") : iconSource
    property bool failed: false
    onDesiredSourceChanged: failed = false
    source: failed ? Qt.resolvedUrl("icons/application.svg") : desiredSource
    onStatusChanged: if (status === Image.Error && !failed) failed = true

    // Magnification and fractional output scales shrink the same texture by
    // different amounts. Linear filtering alone undersamples curved edges and
    // fine details. Mipmaps keep those transitions stable without recolouring
    // or adding a mask around the original artwork.
    mipmap: true
    smooth: true
    fillMode: Image.PreserveAspectFit
    sourceSize.width: 192
    sourceSize.height: 192

    // Both appearances of Calendar are background-only assets. Keep its live
    // labels here so Dock and launcher always render the complete icon.
    Text {
        visible: root.variantName === "calendar"
        anchors.horizontalCenter: parent.horizontalCenter
        y: parent.height * 0.14
        text: Qt.formatDateTime(root.calendarDate, "ddd")
        font.family: Theme.font; font.pixelSize: parent.height * 0.17; font.weight: Font.DemiBold
        color: root.usingDarkVariant ? "#f16d68" : "#d34743"
    }
    Text {
        visible: root.variantName === "calendar"
        anchors.horizontalCenter: parent.horizontalCenter
        y: parent.height * 0.29
        text: Qt.formatDateTime(root.calendarDate, "d")
        font.family: Theme.font; font.pixelSize: parent.height * 0.55
        color: root.usingDarkVariant ? "#f5f5f7" : "#171717"
    }
}

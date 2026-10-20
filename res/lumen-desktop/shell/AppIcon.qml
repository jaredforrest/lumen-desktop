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
    readonly property bool calendarFallback:failed && variantName === "calendar"
    onDesiredSourceChanged: failed = false
    source: calendarFallback ? "" : failed || String(desiredSource)==="" ? Qt.resolvedUrl("icons/application.svg") : desiredSource
    onStatusChanged: if (status === Image.Error && !failed) {
        const expected=String(desiredSource);
        Qt.callLater(()=>{if(root.status===Image.Error && String(root.desiredSource)===expected)root.failed=true;});
    }

    // Magnification and fractional output scales shrink the same texture by
    // different amounts. Linear filtering alone undersamples curved edges and
    // fine details. Mipmaps keep those transitions stable without recolouring
    // or adding a mask around the original artwork.
    mipmap: true
    smooth: true
    fillMode: Image.PreserveAspectFit
    sourceSize.width: 192
    sourceSize.height: 192

    Rectangle {
        visible:root.calendarFallback
        x:root.width*3/68;y:root.height*3/68
        width:root.width*62/68;height:root.height*62/68;radius:root.width*15/68
        color:root.usingDarkVariant?"#45454b":"#f5f5f7"
        border.color:root.usingDarkVariant?"#6c6c74":"#b9b9c1";border.width:Math.max(1,root.width/68)
    }

    // Both appearances of Calendar are background-only assets. Keep its live
    // labels here so Dock and launcher always render the complete icon.
    Text {
        visible: root.variantName === "calendar"
        anchors.horizontalCenter: parent.horizontalCenter
        y: parent.height * 0.14
        text: root.calendarDate.toLocaleDateString(Qt.locale(), "ddd")
        font.family: Theme.font; font.pixelSize: parent.height * 0.17; font.weight: Font.DemiBold
        color: root.usingDarkVariant ? "#f16d68" : "#b93931"
    }
    Text {
        visible: root.variantName === "calendar"
        anchors.horizontalCenter: parent.horizontalCenter
        y: parent.height * 0.29
        text: root.calendarDate.toLocaleDateString(Qt.locale(), "d")
        font.family: Theme.font; font.pixelSize: parent.height * 0.55
        color: root.usingDarkVariant ? "#f5f5f7" : "#171717"
    }
}

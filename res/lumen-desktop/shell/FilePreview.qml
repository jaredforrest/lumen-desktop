import QtQuick
import QtQuick.Controls
import QtMultimedia

Item {
    id: root
    property var result: ({})
    property bool active: visible
    property real zoom: 1
    property bool loading: false
    property var resourceRetirement:null
    readonly property bool playing: player.playing
    readonly property int position: player.position
    readonly property string playbackError: player.errorString
    readonly property bool media: result.type === "media"
    readonly property int page: result.page || 1
    readonly property int pages: result.pages || 1
    signal pageRequested(int number)
    onResultChanged: zoom = 1
    onActiveChanged: if (!active) player.stop()
    function togglePlayback() { if (player.playing) player.pause(); else player.play(); }
    function timeLabel(ms) { const s=Math.floor(ms/1000); return Math.floor(s/60)+":"+(s%60<10?"0":"")+(s%60); }

    Flickable {
        id: picture; visible: root.result.type === "image"
        anchors { top: parent.top; left: parent.left; right: parent.right; bottom: controls.top; bottomMargin: 8 }
        clip: true; contentWidth: width*root.zoom; contentHeight: height*root.zoom
        boundsBehavior: Flickable.StopAtBounds
        ScrollBar.vertical: DesktopScrollBar {}
        ScrollBar.horizontal: DesktopScrollBar {}
        Image {
            id:previewImage
            width: picture.contentWidth; height: picture.contentHeight
            source: root.active && picture.visible ? root.result.uri || "" : ""
            sourceSize: Qt.size(2400,2400); autoTransform: true; fillMode: Image.PreserveAspectFit; asynchronous: true
            Accessible.name: root.result.title || "File preview"
            PanelLabel { anchors.centerIn: parent; visible: parent.status === Image.Error; text: "Preview unavailable" }
        }
    }
    MediaPlayer {
        id: player
        source: root.active && root.media ? root.result.uri || "" : ""
        audioOutput: AudioOutput { volume: 1 }
        videoOutput: video
    }
    PreviewResourceConsumer {
        manager:root.resourceRetirement
        sourceObjects:[previewImage,player]
        mediaObjects:[player]
    }
    VideoOutput {
        id: video; visible: root.media && player.hasVideo
        anchors { top: parent.top; left: parent.left; right: parent.right; bottom: controls.top; bottomMargin: 8 }
        fillMode: VideoOutput.PreserveAspectFit
    }
    ScrollView {
        visible: root.result.type !== "image" && (!root.media || !player.hasVideo)
        anchors { top: parent.top; left: parent.left; right: parent.right; bottom: controls.top; bottomMargin: 8 }
        TextArea {
            readOnly: true; selectByMouse: true; wrapMode: TextEdit.Wrap
            text: player.errorString || root.result.text || root.result.error || "Loading preview…"
            color: Theme.colors.text; font.family: Theme.font; font.pixelSize: 13
            Accessible.name: "Preview text"; background: Rectangle { color: "transparent" }
        }
    }
    Row {
        id: controls; anchors.bottom: parent.bottom; anchors.horizontalCenter: parent.horizontalCenter
        height: 30; spacing: 6
        DialogButton { visible: root.pages>1; width: 28; text: "‹"; Accessible.name: "Previous PDF page"; enabled: root.page>1 && !root.loading; onClicked: root.pageRequested(root.page-1) }
        PanelLabel { visible: root.pages>1; height: 28; verticalAlignment: Text.AlignVCenter; text: root.page+" / "+root.pages; Accessible.name: "Page "+root.page+" of "+root.pages }
        DialogButton { visible: root.pages>1; width: 28; text: "›"; Accessible.name: "Next PDF page"; enabled: root.page<root.pages && !root.loading; onClicked: root.pageRequested(root.page+1) }
        DialogButton { visible: root.result.type==="image"; width: 28; text: "−"; Accessible.name: "Zoom out"; enabled: root.zoom>1; onClicked: root.zoom=Math.max(1,root.zoom-.5) }
        DialogButton { visible: root.result.type==="image"; width: 50; text: Math.round(root.zoom*100)+"%"; Accessible.name: "Fit preview"; onClicked: root.zoom=1 }
        DialogButton { visible: root.result.type==="image"; width: 28; text: "+"; Accessible.name: "Zoom in"; enabled: root.zoom<3; onClicked: root.zoom=Math.min(3,root.zoom+.5) }
        DialogButton { visible: root.media; width: 65; text: player.playing?"Pause":"Play"; enabled: !player.errorString; onClicked: root.togglePlayback() }
        Slider {
            visible: root.media; width: Math.max(70,Math.min(200,root.width-200)); height: 28
            from: 0; to: Math.max(1,player.duration); value: player.position; enabled: player.seekable
            Accessible.name: "Playback position"; onMoved: player.position=value
            background: Rectangle { x:parent.leftPadding; y:(parent.height-height)/2; width:parent.availableWidth; height:4; radius:2; color:Theme.hairline
                Rectangle {width:parent.width*parent.parent.visualPosition;height:parent.height;radius:2;color:Theme.colors.accent}
            }
            handle: Rectangle {x:parent.leftPadding+parent.visualPosition*(parent.availableWidth-width);y:(parent.height-height)/2;width:12;height:12;radius:6;color:Theme.colors.surface;border.color:parent.activeFocus?Theme.colors.accent:Theme.outline}
        }
        PanelLabel { visible: root.media; height: 28; verticalAlignment: Text.AlignVCenter; font.pixelSize: 11; text: root.timeLabel(player.position)+" / "+root.timeLabel(player.duration) }
    }
}

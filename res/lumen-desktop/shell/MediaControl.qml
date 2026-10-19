import QtQuick
import QtQuick.Controls
import Quickshell.Services.Mpris

PanelSurface {
    id: root
    radius: 26
    implicitHeight:mediaContents.implicitHeight+26
    property bool expanded:false
    property string selectedPlayer:""
    signal expandRequested()
    function seekTo(seconds){if(player && player.canSeek && player.positionSupported && player.lengthSupported && Number.isFinite(seconds))player.position=Math.max(0,Math.min(player.length,seconds));}
    function duration(value){const s=Math.max(0,Math.floor(value));return Math.floor(s/60)+":"+String(s%60).padStart(2,"0");}
    property var players: Mpris.players.values.filter(p => p.dbusName !== "org.mpris.MediaPlayer2.playerctld")
    readonly property var player: players.find(p=>p.dbusName===selectedPlayer) || players.find(p => p.isPlaying) || players[0] || null
    DialogButton {visible:!root.expanded;x:parent.width-34;y:8;width:26;height:22;text:"•••";Accessible.name:"Expand Now Playing";onClicked:root.expandRequested()}
    Timer {interval:1000;repeat:true;running:root.visible && root.expanded && !!root.player && root.player.isPlaying;onTriggered:root.player.positionChanged()}
    Column {
        id:mediaContents
        x: 16; y: 13; width: parent.width - 32; spacing: 5
        DesktopComboBox {visible:root.expanded && root.players.length>1;width:parent.width;model:root.players.map(p=>p.identity);Accessible.name:"Media source";currentIndex:root.players.indexOf(root.player);onActivated:index=>root.selectedPlayer=root.players[index].dbusName}
        Rectangle {
            width: 32; height: 32; radius: 7; color: Theme.colors.surface
            SystemGlyph { anchors.centerIn: parent; name: "music"; width: 20; height: 20 }
            Image { anchors.fill: parent; source: root.player ? root.player.trackArtUrl : ""; fillMode: Image.PreserveAspectCrop; asynchronous: true }
        }
        PanelLabel { width: parent.width; text: root.player ? root.player.trackTitle || root.player.identity : "Not Playing"; elide: Text.ElideRight; font.pixelSize:Theme.textSize(11); font.weight: Font.DemiBold }
        PanelLabel { width: parent.width; text: root.player ? root.player.trackArtist || root.player.identity : ""; elide: Text.ElideRight; font.pixelSize:Theme.textSize(10); color: Theme.colors.muted }
        SystemSlider {visible:root.expanded && !!root.player && root.player.lengthSupported;width:parent.width;from:0;to:root.player?Math.max(1,root.player.length):1;value:root.player?root.player.position:0;enabled:!!root.player && root.player.canSeek && root.player.positionSupported;Accessible.name:"Playback position";onMoved:root.seekTo(value)}
        PanelLabel {visible:root.expanded && !!root.player && root.player.lengthSupported;width:parent.width;text:root.player?root.duration(root.player.position)+" / "+root.duration(root.player.length):"";font.pixelSize:Theme.textSize(10);color:Theme.colors.muted}
        Row {
            width: parent.width; spacing: 9
            Repeater {
                model: ["previous", "play", "next"]
                AbstractButton {
                    required property string modelData
                    width: 28; height: 24
                    enabled: !!root.player && root.player.canControl && (modelData === "previous" ? root.player.canGoPrevious : modelData === "next" ? root.player.canGoNext : root.player.canTogglePlaying)
                    opacity: enabled ? 1 : 0.35
                    Accessible.name: modelData === "play" ? (root.player && root.player.isPlaying ? "Pause" : "Play") : modelData === "next" ? "Next track" : "Previous track"
                    contentItem: Item {
                        SystemGlyph { anchors.centerIn: parent; width: 18; height: 18; name: modelData; visible: modelData !== "play" || !root.player || !root.player.isPlaying }
                        Row { anchors.centerIn: parent; spacing: 4; visible: modelData === "play" && !!root.player && root.player.isPlaying; Repeater { model: 2; Rectangle { width: 4; height: 14; radius: 1; color: Theme.colors.text } } }
                    }
                    onClicked: { if (modelData === "previous") root.player.previous(); else if (modelData === "next") root.player.next(); else root.player.togglePlaying(); }
                }
            }
        }
    }
}

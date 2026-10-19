import QtQuick
import QtQuick.Controls
import Quickshell.Services.Mpris

PanelSurface {
    id: root
    Accessible.role:Accessible.Pane
    Accessible.name:"Now Playing"
    Accessible.description:root.player?"Media controls for "+root.player.identity:"No media player available"
    radius: 26
    implicitHeight:mediaContents.implicitHeight+26
    property bool expanded:false
    property string selectedPlayer:""
    signal expandRequested()
    function live(target){return !!target && root.player===target && root.players.includes(target);}
    function intent(operation){const target=root.player;return {target:target,operation:operation==="play"?(target && target.isPlaying?"pause":"play"):operation};}
    function dispatch(value){const target=value.target;if(!live(target) || !target.canControl)return;const op=value.operation;if(op==="previous" && target.canGoPrevious)target.previous();else if(op==="next" && target.canGoNext)target.next();else if(op==="play" && target.canPlay)target.play();else if(op==="pause" && target.canPause)target.pause();}
    function seekTo(target,track,seconds){if(live(target) && target.uniqueId===track && target.canControl && target.canSeek && target.positionSupported && target.lengthSupported && Number.isFinite(seconds))target.position=Math.max(0,Math.min(target.length,seconds));}
    function setPlayerVolume(target,value){if(live(target) && target.canControl && target.volumeSupported && Number.isFinite(value))target.volume=Math.max(0,Math.min(1,value));}
    function duration(value){const s=Math.max(0,Math.floor(value));return Math.floor(s/60)+":"+String(s%60).padStart(2,"0");}
    property var players: Mpris.players.values.filter(p => p.dbusName !== "org.mpris.MediaPlayer2.playerctld")
    readonly property var player: players.find(p=>p.dbusName===selectedPlayer) || players.find(p => p.isPlaying) || players[0] || null
    function sourceChoices(){return root.players.map(player=>({player:player,label:player.identity}));}
    onPlayersChanged:if(sourceSelector && !sourceSelector.popup.visible)sourceSelector.choices=root.sourceChoices()
    DialogButton {visible:!root.expanded;x:parent.width-34;y:8;width:26;height:22;text:"•••";Accessible.name:"Expand Now Playing";onClicked:root.expandRequested()}
    Timer {interval:1000;repeat:true;running:root.visible && root.expanded && !!root.player && root.player.isPlaying;onTriggered:root.player.positionChanged()}
    Column {
        id:mediaContents
        x: 16; y: 13; width: parent.width - 32; spacing: 5
        DesktopComboBox {id:sourceSelector;objectName:"media-source";property var choices:[];visible:root.expanded && root.players.length>1;width:parent.width;model:choices.map(row=>row.label);Accessible.name:"Media source";currentIndex:choices.findIndex(row=>row.player===root.player);onActivated:index=>{const chosen=choices[index]?choices[index].player:null;if(chosen && root.players.includes(chosen))root.selectedPlayer=chosen.dbusName;}Component.onCompleted:choices=root.sourceChoices()
            Connections {target:sourceSelector.popup;function onClosed(){Qt.callLater(()=>sourceSelector.choices=root.sourceChoices());}}
        }
        Rectangle {
            width: 32; height: 32; radius: 7; color: Theme.colors.surface
            SystemGlyph { anchors.centerIn: parent; name: "music"; width: 20; height: 20 }
            Image { anchors.fill: parent; source: root.player ? root.player.trackArtUrl : ""; fillMode: Image.PreserveAspectCrop; asynchronous: true }
        }
        PanelLabel {objectName:"media-title"; width: parent.width; text: root.player ? root.player.trackTitle || root.player.identity : "Not Playing"; elide: Text.ElideRight; font.pixelSize:Theme.textSize(11); font.weight: Font.DemiBold }
        PanelLabel {objectName:"media-artist"; width: parent.width; text: root.player ? root.player.trackArtist || root.player.identity : ""; elide: Text.ElideRight; font.pixelSize:Theme.textSize(10); color: Theme.colors.muted }
        SystemSlider {objectName:"media-position";property var heldPlayer:null;property int heldTrack:0;visible:root.expanded && !!root.player && root.player.lengthSupported;width:parent.width;from:0;to:root.player?Math.max(1,root.player.length):1;value:root.player?root.player.position:0;enabled:!!root.player && root.player.canControl && root.player.canSeek && root.player.positionSupported;Accessible.name:"Playback position";onPressedChanged:if(pressed){heldPlayer=root.player;heldTrack=heldPlayer.uniqueId;}onMoved:root.seekTo(pressed?heldPlayer:root.player,pressed?heldTrack:root.player.uniqueId,value)}
        PanelLabel {visible:root.expanded && !!root.player && root.player.lengthSupported;width:parent.width;text:root.player?root.duration(root.player.position)+" / "+root.duration(root.player.length):"";font.pixelSize:Theme.textSize(10);color:Theme.colors.muted}
        PanelLabel {visible:root.expanded && !!root.player;width:parent.width;text:root.player && root.player.volumeSupported?"Player volume":"Player volume unavailable";font.pixelSize:Theme.textSize(10);color:Theme.colors.muted}
        SystemSlider {objectName:"media-volume";property var heldPlayer:null;visible:root.expanded && !!root.player;width:parent.width;from:0;to:1;value:root.player && root.player.volumeSupported?root.player.volume:0;enabled:!!root.player && root.player.canControl && root.player.volumeSupported;Accessible.name:"Player volume";onPressedChanged:if(pressed)heldPlayer=root.player;onMoved:root.setPlayerVolume(pressed?heldPlayer:root.player,value)}
        Row {
            width: parent.width; spacing: 9
            Repeater {
                model: ["previous", "play", "next"]
                AbstractButton {
                    required property string modelData
                    objectName:"media-"+modelData
                    property var pendingIntent:null
                    onPressedChanged:if(pressed)pendingIntent=root.intent(modelData)
                    onCanceled:pendingIntent=null
                    width: 28; height: 24
                    enabled: !!root.player && root.player.canControl && (modelData === "previous" ? root.player.canGoPrevious : modelData === "next" ? root.player.canGoNext : root.player.canTogglePlaying)
                    opacity: enabled ? 1 : 0.35
                    Accessible.name: modelData === "play" ? (root.player && root.player.isPlaying ? "Pause" : "Play") : modelData === "next" ? "Next track" : "Previous track"
                    contentItem: Item {
                        SystemGlyph { anchors.centerIn: parent; width: 18; height: 18; name: modelData; visible: modelData !== "play" || !root.player || !root.player.isPlaying }
                        Row { anchors.centerIn: parent; spacing: 4; visible: modelData === "play" && !!root.player && root.player.isPlaying; Repeater { model: 2; Rectangle { width: 4; height: 14; radius: 1; color: Theme.colors.text } } }
                    }
                    onClicked:{const value=pendingIntent || root.intent(modelData);pendingIntent=null;root.dispatch(value);}
                }
            }
        }
    }
}

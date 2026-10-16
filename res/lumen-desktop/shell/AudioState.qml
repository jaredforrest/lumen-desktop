import QtQuick
import Quickshell.Services.Pipewire

// PipeWire events keep keyboard, mixer and slider changes on the same state.
Item {
    id: audio
    readonly property var outputs: Pipewire.nodes.values.filter(n => n.isSink && !n.isStream && n.audio)
    readonly property var inputs:Pipewire.nodes.values.filter(n=>!n.isSink && !n.isStream && n.audio)
    readonly property var streams:Pipewire.nodes.values.filter(n=>n.isStream && n.audio)
    readonly property var source:Pipewire.defaultAudioSource
    readonly property bool inputAvailable:Pipewire.ready && source!==null && source.ready && source.audio!==null && Number.isFinite(source.audio.volume)
    function selectInput(node){if(inputs.includes(node)){Pipewire.preferredDefaultAudioSource=node;}}
    readonly property var sink: Pipewire.defaultAudioSink
    readonly property bool available: Pipewire.ready && sink !== null && sink.ready && sink.audio !== null && Number.isFinite(sink.audio.volume)
    readonly property real level: available ? Math.max(0, Math.min(1, sink.audio.volume)) : 0
    readonly property bool muted: available ? sink.audio.muted : false
    property bool editing: false
    readonly property string volume: available ? "Volume: " + level.toFixed(3) + (muted ? " [MUTED]" : "") : ""
    PwObjectTracker { objects: audio.outputs.concat(audio.inputs,audio.streams) }
    property string error:""
    function setVolume(value) { if (available) {sink.audio.volume = Math.max(0, Math.min(1, value));} }
    function toggleMute() { if (available) {sink.audio.muted = !sink.audio.muted;} }
    function selectOutput(node) { if (outputs.includes(node)) {Pipewire.preferredDefaultAudioSink = node;} }
}

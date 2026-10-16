pragma Singleton
import QtQuick
import Quickshell
import Quickshell.Io
Singleton {
    id: root
    property var preferences: ({mode:"auto",accent:"blue",iconAppearance:"auto",dockMagnification:true,reduceMotion:false})
    property string font: "Interlude Variable"
    property var colors: ({background:"#f2f2f4",surface:"#ffffff",field:"#e5e5e8",text:"#202023",muted:"#6b6b70",border:"#c8c8ce",hover:"#dedee3",accent:"#007aff",selectedText:"#ffffff"})
    readonly property color backgroundTone: colors.background
    readonly property bool dark: backgroundTone.hslLightness < 0.5
    readonly property string iconAppearance: preferences.iconAppearance || "auto"
    readonly property bool nativeWindowFrame: Quickshell.env("LUMEN_NATIVE_WINDOW_CONTROLS") === "1" && !unifiedOwnedHeaders
    readonly property bool unifiedOwnedHeaders: Quickshell.env("LUMEN_UNIFIED_OWNED_HEADERS") === "1"
    readonly property bool nativeWindowActions: Quickshell.env("LUMEN_NATIVE_WINDOW_ACTIONS") === "1"
    readonly property bool compositorEffects: Quickshell.env("LUMEN_COMPOSITOR_EFFECTS") === "1"
    readonly property int barHeight: 28
    readonly property color outline: dark ? "#505054" : "#ccccd0"
    readonly property color hairline: dark ? "#404044" : "#dedee2"
    readonly property color sidebar: dark ? "#242426" : "#e8e8ec"
    readonly property color content: dark ? "#222224" : "#ffffff"
    readonly property color sidebarMaterial: compositorEffects && !preferences.reduceTransparency ? Qt.rgba(sidebar.r,sidebar.g,sidebar.b,0.86) : sidebar
    readonly property color popoverMaterial: compositorEffects && !preferences.reduceTransparency ? Qt.rgba(backgroundTone.r,backgroundTone.g,backgroundTone.b,0.94) : backgroundTone
    readonly property color selection: dark ? "#414145" : "#dedee3"
    readonly property int menuRowHeight: 23
    readonly property int menuRadius: 10
    FileView {
        id: file
        path: Quickshell.env("LUMEN_APPEARANCE_STATE") || ((Quickshell.env("XDG_STATE_HOME") || Quickshell.env("HOME")+"/.local/state")+"/lumen-desktop/appearance.json")
        watchChanges: true
        onFileChanged: reload()
        onLoaded: {try {const data=JSON.parse(text());root.colors=data.palette;root.font=data.font;root.preferences=data.preferences || root.preferences;}catch(e){console.warn("Appearance file:",e);}}
    }
}

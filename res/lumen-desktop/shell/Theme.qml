pragma Singleton
import QtQuick
import Quickshell
import Quickshell.Io
Singleton {
    id: root
    property var preferences: ({mode:"auto",accent:"blue",iconAppearance:"auto",switchToAppSpace:true,dockMagnification:true,reduceMotion:false})
    property var desktopPreferences: ({showFiles:false,fileSort:"name",fileDescending:false})
    property var windowsPreferences: ({tileMargins:true,tileGap:8,titlebarDoubleClick:"fill",edgeTiling:true,optionTiling:true,topEdgeFill:true,topEdgeOverview:true})
    property var menuBarPreferences: ({order:["battery","sound","focus","search","controls"],hourCycle:"locale",locale:"",fullscreenVisibility:"automatic",showDate:true,showWeekday:true,showSeconds:false})
    property var controlCentrePreferences: ({order:["wifi","bluetooth","sharing","media","displays","appearance","focus","brightness","sound"],hidden:[]})
    property var notificationsPreferences: ({preview:"when_unlocked"})
    property var searchPreferences: ({disabledProviders:[]})
    readonly property real textScale:[1,1.15,1.3,1.5].includes(preferences.textScale)?preferences.textScale:1
    function textSize(base){return Math.round(base*textScale);}
    property string font: "Interlude Variable"
    property var colors: ({background:"#f2f2f4",surface:"#ffffff",field:"#e5e5e8",text:"#202023",muted:"#626267",border:"#c8c8ce",hover:"#dedee3",accent:"#007aff",selectedText:"#000000"})
    readonly property color backgroundTone: colors.background
    readonly property bool dark: backgroundTone.hslLightness < 0.5
    readonly property color errorText: colors.error || (dark ? "#ffaba5" : "#b42318")
    readonly property color successText: colors.success || (dark ? "#55d788" : "#196633")
    readonly property color warningText: colors.warning || (dark ? "#e0b85a" : "#775000")
    readonly property color destructiveFill: colors.destructiveFill || "#b42318"
    readonly property color destructiveText: colors.destructiveText || "#ffffff"
    readonly property string iconAppearance: preferences.iconAppearance || "auto"
    readonly property bool nativeWindowFrame: Quickshell.env("LUMEN_NATIVE_WINDOW_CONTROLS") === "1" && !unifiedOwnedHeaders
    readonly property bool unifiedOwnedHeaders: Quickshell.env("LUMEN_UNIFIED_OWNED_HEADERS") === "1"
    readonly property bool nativeWindowActions: Quickshell.env("LUMEN_NATIVE_WINDOW_ACTIONS") === "1"
    readonly property bool compositorEffects: Quickshell.env("LUMEN_COMPOSITOR_EFFECTS") === "1"
    readonly property int barHeight: 28
    readonly property int sidebarIconSize: preferences.sidebarIconSize === "small" ? 14 : preferences.sidebarIconSize === "large" ? 24 : 18
    readonly property bool increaseContrast: !!preferences.increaseContrast
    readonly property color outline: increaseContrast ? colors.border : dark ? "#505054" : "#ccccd0"
    readonly property color hairline: increaseContrast ? colors.border : dark ? "#404044" : "#dedee2"
    readonly property color sidebar: dark ? "#242426" : "#e8e8ec"
    readonly property color content: dark ? "#222224" : "#ffffff"
    readonly property color sidebarMaterial: compositorEffects && !preferences.reduceTransparency && !increaseContrast ? Qt.rgba(sidebar.r,sidebar.g,sidebar.b,0.86) : sidebar
    readonly property color popoverMaterial: compositorEffects && !preferences.reduceTransparency && !increaseContrast ? Qt.rgba(backgroundTone.r,backgroundTone.g,backgroundTone.b,0.94) : backgroundTone
    readonly property color selection: dark ? "#414145" : "#dedee3"
    readonly property int menuRowHeight: 23
    readonly property int menuRadius: 10
    FileView {
        id: file
        path: Quickshell.env("LUMEN_APPEARANCE_STATE") || ((Quickshell.env("XDG_STATE_HOME") || Quickshell.env("HOME")+"/.local/state")+"/lumen-desktop/appearance.json")
        watchChanges: true
        onFileChanged: reload()
        onLoaded: {try {const data=JSON.parse(text());root.colors=data.palette;root.font=data.font;root.preferences=data.preferences || root.preferences;root.searchPreferences=data.searchPreferences || ({disabledProviders:[]});root.notificationsPreferences=data.notificationsPreferences || ({preview:"when_unlocked"});if(data.menuBarPreferences)root.menuBarPreferences=data.menuBarPreferences;if(data.controlCentrePreferences)root.controlCentrePreferences=data.controlCentrePreferences;if(data.windowsPreferences)root.windowsPreferences=data.windowsPreferences;if(data.desktopPreferences)root.desktopPreferences=data.desktopPreferences;}catch(e){console.warn("Appearance file:",e);}}
    }
}

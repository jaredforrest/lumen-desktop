import QtQuick
import Quickshell
import Quickshell.Wayland
PanelWindow {
    id:root
    required property var shell
    required property string outputName
    required property real outputWidth
    required property real outputHeight
    readonly property var service:shell.notifications
    visible:service.available && !service.dnd && service.banners.length>0 && shell.popupOwner==="" && (service.bannerOutput===outputName || !Quickshell.screens.some(s=>s.name===service.bannerOutput) && Quickshell.screens[0].name===outputName)
    anchors {top:true;right:true}
    margins {top:Theme.barHeight+4;right:4}
    implicitWidth:Math.min(368,outputWidth)
    implicitHeight:Math.min(stack.height+24,Math.max(1,outputHeight-Theme.barHeight-110))
    exclusionMode:ExclusionMode.Ignore
    exclusiveZone:0;color:"transparent"
    WlrLayershell.namespace:"lumen-notifications"
    WlrLayershell.layer:WlrLayer.Overlay
    WlrLayershell.keyboardFocus:WlrKeyboardFocus.None
    Column {
        id:stack;x:12;y:12;width:Math.max(1,parent.width-24);spacing:10
        Repeater {
            model:root.service.banners
            NotificationCard {
                required property var modelData
                opacity:0
                NumberAnimation on opacity {from:0;to:1;duration:Theme.preferences.reduceMotion?0:120;running:true}
                width:stack.width;notification:modelData;service:root.service;compact:true
                onDetailsRequested:root.shell.showMenu("Date",root.outputName)
            }
        }
    }
}

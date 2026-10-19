import QtQuick
import Quickshell
import Quickshell.Wayland
PanelWindow {
    id:root
    required property var shell
    required property string outputName
    readonly property real widgetWidth:widget.width
    readonly property real widgetHeight:widget.height
    Component.onCompleted:root.shell.widgetSurfaces[outputName]=root
    Component.onDestruction:delete root.shell.widgetSurfaces[outputName]
    readonly property var preferences:shell.widgets.preferences
    readonly property var position:preferences.positions[outputName] || {x:0.02,y:0.03}
    readonly property bool monochrome:preferences.desktopColor==="monochrome" || preferences.desktopColor==="auto" && (shell.exportedMenu.latestState || shell.exportedMenu.state).windowId>0
    visible:preferences.desktopVisible
    anchors {left:true;right:true;top:true;bottom:true}
    color:"transparent";exclusiveZone:0
    exclusionMode:ExclusionMode.Ignore
    WlrLayershell.layer:WlrLayer.Bottom
    WlrLayershell.namespace:"lumen-desktop-widgets"
    WlrLayershell.keyboardFocus:WlrKeyboardFocus.None
    mask:Region {item:widget}
    Item {
        id:widget;width:Math.min(344,root.width-24);height:Math.min(calendar.implicitHeight,root.height-Theme.barHeight-100)
        x:Math.round(root.position.x*Math.max(0,root.width-width))
        y:Math.max(Theme.barHeight+10,Math.round(root.position.y*Math.max(0,root.height-height-80)))
        opacity:root.monochrome?0.84:1
        HoverHandler {id:widgetHover}
        Rectangle {z:2;x:parent.width-30;y:4;width:26;height:20;radius:7;visible:widgetHover.hovered || dragMouse.pressed;color:Theme.colors.surface
            PanelLabel {anchors.centerIn:parent;text:"⠿";color:Theme.colors.muted}
            MouseArea {id:dragMouse;anchors.fill:parent;drag.target:widget;drag.minimumX:8;drag.maximumX:Math.max(8,root.width-widget.width-8);drag.minimumY:Theme.barHeight+10;drag.maximumY:Math.max(Theme.barHeight+10,root.height-widget.height-80)
                onReleased:{root.shell.widgets.setPosition(root.outputName,widget.x/Math.max(1,root.width-widget.width),widget.y/Math.max(1,root.height-widget.height-80));widget.x=Qt.binding(()=>Math.round(root.position.x*Math.max(0,root.width-widget.width)));widget.y=Qt.binding(()=>Math.max(Theme.barHeight+10,Math.round(root.position.y*Math.max(0,root.height-widget.height-80))));}
            }
        }
        Flickable {width:parent.width;height:parent.height;contentWidth:width;contentHeight:calendar.implicitHeight;clip:true;boundsBehavior:Flickable.StopAtBounds
            CalendarCard {id:calendar;width:widget.width;today:root.shell.currentDate;compact:true;agendaOnly:root.preferences.desktopStyle==="agenda";monochrome:root.monochrome;reduceTransparency:root.preferences.reduceTransparency || !!Theme.preferences.reduceTransparency;active:root.visible
                onOpenCalendar:root.shell.openCalendarAt(selected,null)
                onEventRequested:(event,day)=>root.shell.openCalendarAt(day,event)
                onAccountsRequested:root.shell.openSettings("Internet Accounts")
            }
        }
    }
}

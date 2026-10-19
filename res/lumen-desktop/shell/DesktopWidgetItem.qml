import QtQuick
import QtQuick.Controls
import Quickshell
import "WidgetModel.js" as Model
import "ClockFormat.js" as Clock
FocusScope {
    id:root
    required property var shell
    required property var descriptor
    required property string outputName
    required property int widgetIndex
    required property real availableWidth
    required property real availableHeight
    required property bool active
    property bool monochrome:false
    property var preview:null
    property var gesture:null
    readonly property var geometry:Model.placed(descriptor,outputName,widgetIndex,availableWidth,availableHeight,Theme.barHeight)
    readonly property string label:descriptor.type==="clock"?"Local time":"Calendar"
    x:preview?preview.x:geometry.x;y:preview?preview.y:geometry.y;width:geometry.width;height:geometry.height
    opacity:monochrome?0.84:1
    function cancelMove(){gesture=null;preview=null;}
    function beginMove(point){gesture={point:point,x:x,y:y};preview={x:x,y:y};}
    function moveTo(x,y){if(!gesture)return;preview=Model.placed(descriptor,outputName,widgetIndex,availableWidth,availableHeight,Theme.barHeight,{x:x/Math.max(1,availableWidth-width),y:y/Math.max(1,availableHeight-height-80)});}
    function commitMove(){if(!gesture || !preview)return;const p=preview;gesture=null;shell.widgets.positionWidget(descriptor.id,outputName,p.x/Math.max(1,availableWidth-width),p.y/Math.max(1,availableHeight-height-80));preview=null;}
    onDescriptorChanged:cancelMove()
    onAvailableWidthChanged:cancelMove()
    onAvailableHeightChanged:cancelMove()
    Connections {target:root.shell.widgets;function onPreferencesChanged(){root.cancelMove();}}
    PanelSurface {anchors.fill:parent;radius:18;clip:true
        Flickable {anchors.fill:parent;anchors.topMargin:26;visible:root.descriptor.type==="calendar";contentWidth:width;contentHeight:calendar.implicitHeight;clip:true;boundsBehavior:Flickable.StopAtBounds;ScrollBar.vertical:DesktopScrollBar {}
            CalendarCard {id:calendar;width:parent.width;today:root.shell.currentDate;compact:true;agendaOnly:root.descriptor.size==="small" || root.descriptor.style==="agenda";monochrome:root.monochrome;reduceTransparency:root.shell.widgets.preferences.reduceTransparency || !!Theme.preferences.reduceTransparency;active:root.active && root.descriptor.type==="calendar"
                onOpenCalendar:root.shell.openCalendarAt(selected,null)
                onEventRequested:(event,day)=>root.shell.openCalendarAt(day,event)
                onAccountsRequested:root.shell.openSettings("Internet Accounts")
            }
        }
        AbstractButton {id:clock;visible:root.descriptor.type==="clock";x:12;y:30;width:parent.width-24;height:parent.height-38
            readonly property string timeText:root.shell.currentDate.toLocaleTimeString(Qt.locale(Theme.menuBarPreferences.locale || ""),Clock.timePattern(Qt.locale(Theme.menuBarPreferences.locale || ""),{hourCycle:root.descriptor.hourCycle,showSeconds:false}))
            Accessible.name:"Local time "+timeText+". Open date panel"
            contentItem:Column {spacing:6
                PanelLabel {width:parent.width;text:clock.timeText;font.pixelSize:Theme.textSize(root.descriptor.size==="small"?28:42);fontSizeMode:Text.Fit;minimumPixelSize:16;horizontalAlignment:Text.AlignHCenter}
                PanelLabel {width:parent.width;text:Qt.formatDate(root.shell.currentDate,Qt.locale().dateFormat(Locale.ShortFormat));wrapMode:Text.Wrap;horizontalAlignment:Text.AlignHCenter;color:Theme.colors.muted}
                PanelLabel {width:parent.width;visible:root.descriptor.size!=="small";text:"Open date panel";font.pixelSize:Theme.textSize(11);horizontalAlignment:Text.AlignHCenter;color:Theme.colors.muted}
            }
            background:Rectangle {radius:12;color:clock.hovered || clock.activeFocus?Theme.colors.hover:"transparent";border.width:clock.activeFocus?2:0;border.color:Theme.colors.accent}
            onClicked:root.shell.showMenu("Date",root.outputName)
        }
    }
    PanelLabel {x:12;y:6;width:parent.width-70;text:root.label;font.pixelSize:Theme.textSize(11);font.weight:Font.DemiBold;color:Theme.colors.muted}
    Rectangle {id:handle;x:parent.width-44;y:3;width:38;height:23;radius:7;color:handle.activeFocus?Theme.colors.hover:"transparent";border.width:handle.activeFocus?2:0;border.color:Theme.colors.accent;activeFocusOnTab:true
        onActiveFocusChanged:if(!activeFocus)root.cancelMove()
        Accessible.role:Accessible.Button;Accessible.name:"Move "+root.label+" widget. Space to begin, arrows to move, Return to place, Escape to cancel"
        Accessible.onPressAction:root.gesture?root.commitMove():root.beginMove(null)
        PanelLabel {anchors.centerIn:parent;text:root.gesture?"↔":"⠿";color:Theme.colors.muted}
        Keys.onPressed:event=>{
            if(event.key===Qt.Key_Escape && root.gesture){root.cancelMove();event.accepted=true;}
            else if(event.key===Qt.Key_Space || event.key===Qt.Key_Return){if(root.gesture)root.commitMove();else root.beginMove(null);event.accepted=true;}
            else if(root.gesture && [Qt.Key_Left,Qt.Key_Right,Qt.Key_Up,Qt.Key_Down].includes(event.key)){const step=event.modifiers & Qt.ShiftModifier?1:10;root.moveTo(root.x+(event.key===Qt.Key_Left?-step:event.key===Qt.Key_Right?step:0),root.y+(event.key===Qt.Key_Up?-step:event.key===Qt.Key_Down?step:0));event.accepted=true;}
        }
        MouseArea {id:drag;anchors.fill:parent
            onPressed:mouse=>{handle.forceActiveFocus();root.beginMove(drag.mapToItem(root.parent,mouse.x,mouse.y));}
            onPositionChanged:mouse=>{if(!pressed || !root.gesture || !root.gesture.point)return;const point=drag.mapToItem(root.parent,mouse.x,mouse.y);root.moveTo(root.gesture.x+point.x-root.gesture.point.x,root.gesture.y+point.y-root.gesture.point.y);}
            onReleased:root.commitMove()
            onCanceled:root.cancelMove()
        }
    }
    MouseArea {anchors.fill:parent;acceptedButtons:Qt.RightButton;onClicked:{root.shell.showMenu("Date",root.outputName);root.shell.widgets.edit();}}
}

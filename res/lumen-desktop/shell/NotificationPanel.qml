import QtQuick
import QtQuick.Controls
import Quickshell
import Quickshell.Io
Column {
    id:root
    required property var shell
    required property date today
    property bool editingWidgets:false
    property var preferences:({calendarVisible:true,calendarFirst:false,style:"month",reduceTransparency:false})
    property string preferenceError:""
    width:344;spacing:14
    function status(){return {width:width,height:height,style:preferences.style,calendarVisible:preferences.calendarVisible,calendarFirst:preferences.calendarFirst,selected:calendar.iso(calendar.selected),followingToday:calendar.followingToday,events:calendar.events.length,busy:calendar.busy,preferenceError:preferenceError};}
    function setPreference(key,value){const next=Object.assign({},preferences);next[key]=value;preferences=next;preferencesFile.setText(JSON.stringify(next));}
    FileView {
        id:preferencesFile
        path:(Quickshell.env("XDG_STATE_HOME") || Quickshell.env("HOME")+"/.local/state")+"/lumen-desktop/session/notification-centre.json"
        printErrors:false;atomicWrites:true;watchChanges:true
        onFileChanged:reload()
        onLoaded:{try{const data=JSON.parse(text());root.preferences={calendarVisible:data.calendarVisible!==false,calendarFirst:data.calendarFirst===true,style:data.style==="agenda"?"agenda":"month",reduceTransparency:data.reduceTransparency===true};}catch(e){root.preferenceError="Widget preferences could not be read.";}}
        onSaved:root.preferenceError=""
        onSaveFailed:root.preferenceError="Widget changes could not be saved."
    }
    Column {id:before;visible:root.preferences.calendarFirst;width:parent.width}
    NotificationCentre {width:parent.width;service:root.shell.notifications;reduceTransparency:root.preferences.reduceTransparency || !!Theme.preferences.reduceTransparency;onVisibleChanged:if(visible)root.shell.notifications.seen()}
    Column {id:after;visible:!root.preferences.calendarFirst;width:parent.width}
    CalendarCard {
        id:calendar;objectName:"calendar-widget"
        parent:root.preferences.calendarFirst?before:after
        visible:root.preferences.calendarVisible;width:root.width;today:root.today;compact:true;agendaOnly:root.preferences.style==="agenda";reduceTransparency:root.preferences.reduceTransparency || !!Theme.preferences.reduceTransparency
        active:root.visible && visible
        onOpenCalendar:root.shell.openCalendarAt(calendar.selected,null)
        onEventRequested:(event,day)=>root.shell.openCalendarAt(day,event)
    }
    ControlRow {width:parent.width;height:30;text:root.editingWidgets?"Done":"Edit Widgets";onClicked:root.editingWidgets=!root.editingWidgets}
    PanelSurface {
        visible:root.editingWidgets;width:parent.width;implicitHeight:editor.height+28;radius:18
        Column {id:editor;x:14;y:14;width:parent.width-28;spacing:8
            PanelLabel {text:"Calendar widget";font.weight:Font.DemiBold}
            SettingsToggle {width:parent.width;height:36;text:"Show Calendar";checked:root.preferences.calendarVisible;onClicked:root.setPreference("calendarVisible",checked)}
            DesktopComboBox {width:parent.width;model:["Month and agenda","Agenda"];currentIndex:root.preferences.style==="agenda"?1:0;Accessible.name:"Calendar widget layout";onActivated:index=>root.setPreference("style",index===1?"agenda":"month")}
            ControlRow {width:parent.width;text:root.preferences.calendarFirst?"Move Calendar below notifications":"Move Calendar above notifications";onClicked:root.setPreference("calendarFirst",!root.preferences.calendarFirst)}
            SettingsToggle {width:parent.width;height:36;text:"Reduce transparency";checked:root.preferences.reduceTransparency;onClicked:root.setPreference("reduceTransparency",checked)}
            ControlRow {width:parent.width;text:"Calendar accounts…";onClicked:root.shell.openSettings("Internet Accounts")}
        }
    }
    PanelLabel {visible:!!root.preferenceError;width:parent.width;wrapMode:Text.WordWrap;text:root.preferenceError;color:Theme.colors.muted}
}

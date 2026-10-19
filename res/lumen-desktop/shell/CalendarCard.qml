import QtQuick
import QtQuick.Controls
import Quickshell
import Quickshell.Io
import "CalendarLogic.js" as CalendarLogic
PanelSurface {
    id: root
    required property date today
    property date month: new Date(today.getFullYear(),today.getMonth(),1)
    property date selected: new Date(today.getFullYear(),today.getMonth(),today.getDate())
    property var events:[]
    property var monthEvents:[]
    property bool detailsOnly:false
    property bool compact:false
    property bool agendaOnly:false
    property bool reduceTransparency:false
    property bool monochrome:false
    property bool followingToday:true
    property int firstDayOfWeek:Qt.locale().firstDayOfWeek
    property string calendarTimeZone:"Europe/London"
    signal eventRequested(var event, date day)
    signal accountsRequested()
    property string observedToday:""
    onTodayChanged:{const key=iso(today);if(key===observedToday)return;observedToday=key;if(followingToday){selected=new Date(today.getFullYear(),today.getMonth(),today.getDate());month=new Date(today.getFullYear(),today.getMonth(),1);if(loaded && active)refresh();}}
    function goToday(){selectDate(new Date(today));}
    function moveDate(key) {
        let d=new Date(selected);
        if(key===Qt.Key_Left)d.setDate(d.getDate()-1);
        else if(key===Qt.Key_Right)d.setDate(d.getDate()+1);
        else if(key===Qt.Key_Up)d.setDate(d.getDate()-7);
        else if(key===Qt.Key_Down)d.setDate(d.getDate()+7);
        else if(key===Qt.Key_PageUp)d=CalendarLogic.monthShift(d,-1);
        else if(key===Qt.Key_PageDown)d=CalendarLogic.monthShift(d,1);
        else if(key===Qt.Key_Home){goToday();return true;}
        else return false;
        selectDate(d);return true;
    }
    function dayColors(day){return Array.from(new Set(dayEvents(day).map(event=>tint(event)))).slice(0,3);}
    function tint(event){return monochrome?Theme.colors.muted:CalendarLogic.tint(event,Theme.dark);}
    function timeLabel(event){if(event.allDay)return "All day";const d=new Date(event.date+"T"+event.startTime);return isNaN(d.getTime())?event.startTime:Qt.formatTime(d,Qt.locale().timeFormat(Locale.ShortFormat));}
    property bool optionsOpen:false
    property bool active:visible
    signal openCalendar()
    readonly property bool refreshing:background.running
    property string backgroundError:""
    property var markedDays:[]
    property int total:0
    property string error:""
    property bool loaded:false
    property var detail:null
    property bool configured:false
    property var providers:[]
    property bool editing:false
    property var editEvent:null
    property bool deletePending:false
    property string operationId:""
    property string editGeneration:""
    property string mutationWarning:""
    readonly property var writableCalendars: {
        const google=providers.find(provider=>provider.id==="google");
        return google && google.calendars?google.calendars.filter(calendar=>calendar.accessRole==="owner" || calendar.accessRole==="writer"):[];
    }
    function startEdit(event) {
        editEvent=event;editGeneration=(event?event.accountGeneration:(providers.find(provider=>provider.id==="google") || {}).generation) || "";deletePending=false;error="";mutationWarning="";
        operationId=Array.from({length:32},()=>Math.floor(Math.random()*16).toString(16)).join("");
        editTitle.text=event?event.title:"";editLocation.text=event?event.location:"";editNotes.text=event?event.notes:"";
        allDay.checked=event?event.allDay:false;
        editStart.text=event?event.editStart:iso(selected)+"T09:00";
        editEnd.text=event?event.editEnd:iso(selected)+"T10:00";
        calendarChoice.currentIndex=event?Math.max(0,writableCalendars.findIndex(calendar=>calendar.id===event.calendarId)):0;
        editing=true;Qt.callLater(()=>editTitle.forceActiveFocus());
    }
    function setStartHour(hour){editStart.text=iso(selected)+"T"+String(hour).padStart(2,"0")+":00";editEnd.text=iso(selected)+"T"+String(Math.min(23,hour+1)).padStart(2,"0")+":00";}
    function saveEvent() {
        const calendar=writableCalendars[calendarChoice.currentIndex];
        if(!calendar){error="Choose a writable Google calendar.";return;}
        send({action:editEvent?"update":"create",calendarId:editEvent?editEvent.calendarId:calendar.id,
              accountGeneration:editGeneration,eventId:editEvent?editEvent.eventId:"",recurrenceId:editEvent?editEvent.recurrenceId:"",etag:editEvent?editEvent.etag:"",operationId:operationId,
              fields:{title:editTitle.text,location:editLocation.text,notes:editNotes.text,allDay:allDay.checked,start:editStart.text,end:editEnd.text}});
    }
    property string lastSuccess:""
    property string syncError:""
    property string warning:""
    property var request:({})
    property bool refreshPending:false
    property int mutationEpoch:0
    readonly property bool busy:worker.running
    readonly property int weekCount: Math.ceil((CalendarLogic.offset(month,firstDayOfWeek)+new Date(month.getFullYear(),month.getMonth()+1,0).getDate())/7)
    color:compact && Theme.compositorEffects && !root.reduceTransparency?Qt.alpha(Theme.colors.background,.91):Theme.colors.background;radius:22
    implicitHeight:(editing?editor.height:content.height)+36+(compact?34:0)
    function iso(d) {return d.getFullYear()+"-"+String(d.getMonth()+1).padStart(2,"0")+"-"+String(d.getDate()).padStart(2,"0");}
    function send(data) {
        if(worker.running){if(data.action==="list")refreshPending=true;return;}
        if(["create","update","delete"].includes(data.action))++mutationEpoch;
        request=Object.assign({selectedDate:iso(selected),month:iso(month)},data);
        error="";worker.running=true;
    }
    function refresh() {send({action:"list"});}
    function refreshBackground() {if(!background.running && !busy && !editing){background.epoch=mutationEpoch;background.requestMonth=iso(month);background.requestDate=iso(selected);background.running=true;}}
    function dayEvents(d) {const key=iso(d);return monthEvents.filter(e=>e.date<=key && e.endDate>=key);}
    function updateSelection() {events=dayEvents(selected).slice(0,100);total=dayEvents(selected).length;}
    function apply(data) {
        monthEvents=data.monthEvents || data.events || [];events=data.events || [];markedDays=data.days || [];total=data.total || 0;
        if(detail)detail=monthEvents.find(e=>e.id===detail.id) || null;
        loaded=true;configured=data.configured;providers=data.providers || [];calendarTimeZone=data.timezone || calendarTimeZone;lastSuccess=data.lastSuccess || "";syncError=data.syncError || "";warning=data.warning || "";
    }
    function selectDate(d) {followingToday=iso(d)===iso(today);const changed=month.getFullYear()!==d.getFullYear() || month.getMonth()!==d.getMonth();month=new Date(d.getFullYear(),d.getMonth(),1);selected=d;detail=null;if(changed || !loaded){send({action:"list"});}else updateSelection();}
    function changeMonth(delta) {selectDate(CalendarLogic.monthShift(selected,delta));}
    onActiveChanged:if(active)refresh()
    Component.onCompleted:if(active)refresh()
    Column {
        id:content;visible:!root.editing;x:18;y:18;width:parent.width-36;spacing:10
        Row {
            visible:!root.detailsOnly && !root.agendaOnly
            width:parent.width
            PanelLabel {text:Qt.formatDateTime(root.month,"MMMM yyyy");font.weight:Font.DemiBold;width:parent.width-58;height:26;verticalAlignment:Text.AlignVCenter}
            Repeater {
                model:[-1,1]
                delegate:AbstractButton {
                    required property int modelData
                    width:29;height:26;enabled:!root.busy
                    Accessible.name:modelData<0?"Previous month":"Next month"
                    contentItem:PanelLabel {text:parent.modelData<0?"‹":"›";font.pixelSize:20;horizontalAlignment:Text.AlignHCenter;verticalAlignment:Text.AlignVCenter}
                    background:Rectangle {radius:5;color:parent.hovered || parent.activeFocus?Theme.colors.hover:"transparent"}
                    onClicked:root.changeMonth(modelData)
                }
            }
        }
        Grid {
            visible:!root.detailsOnly && !root.agendaOnly
            id:dateGrid;objectName:"calendar-grid"
            width:parent.width;columns:7;rowSpacing:5;focus:root.compact
            activeFocusOnTab:true
            Accessible.role:Accessible.Pane
            Accessible.name:"Calendar, "+Qt.formatDateTime(root.selected,"dddd d MMMM yyyy")
            Keys.onPressed:event=>{if(root.moveDate(event.key))event.accepted=true;}
            Repeater {model:CalendarLogic.weekdayNames(Qt.locale(),root.firstDayOfWeek,Locale.ShortFormat);PanelLabel {required property string modelData;width:parent.width/7;height:18;text:modelData;horizontalAlignment:Text.AlignHCenter;color:Theme.colors.muted;font.pixelSize:11}}
            Repeater {
                model:7*root.weekCount
                delegate:AbstractButton {
                    id:cell
                    objectName:valid?"calendar-day-"+root.iso(dateValue):""
                    required property int index
                    width:parent.width/7;height:30
                    readonly property int day:index-CalendarLogic.offset(root.month,root.firstDayOfWeek)+1
                    readonly property bool valid:day>0 && day<=new Date(root.month.getFullYear(),root.month.getMonth()+1,0).getDate()
                    readonly property date dateValue:new Date(root.month.getFullYear(),root.month.getMonth(),day)
                    readonly property bool current:valid && root.iso(dateValue)===root.iso(root.today)
                    readonly property bool selected:valid && root.iso(dateValue)===root.iso(root.selected)
                    enabled:valid && !root.busy
                    activeFocusOnTab:false
                    Accessible.role:Accessible.Button
                    Accessible.checkable:true
                    Accessible.checked:selected
                    Accessible.name:valid?Qt.formatDateTime(dateValue,"dddd d MMMM yyyy")+(current?", today":"")+(selected?", selected":"")+(root.dayEvents(dateValue).length?", "+root.dayEvents(dateValue).length+" events":""):""
                    onClicked:{root.selectDate(dateValue);dateGrid.forceActiveFocus();}
                    background:Item {
                        Rectangle {anchors.horizontalCenter:parent.horizontalCenter;width:26;height:26;radius:13;color:cell.current?(root.monochrome?Theme.colors.muted:Theme.dark?"#ff453a":"#ff3b30"):cell.selected?(root.monochrome?Theme.colors.muted:Theme.colors.accent):cell.hovered?Theme.colors.hover:"transparent";border.width:cell.activeFocus || (dateGrid.activeFocus && cell.selected)?2:0;border.color:Theme.colors.text}
                        Row {anchors.horizontalCenter:parent.horizontalCenter;y:28;spacing:2;visible:cell.valid;Repeater {model:cell.valid?root.dayColors(cell.dateValue):[];Rectangle {required property string modelData;width:3;height:3;radius:2;color:root.monochrome?Theme.colors.muted:modelData}}}
                    }
                    contentItem:PanelLabel {height:26;text:cell.valid?cell.day:"";color:cell.current || cell.selected?"white":Theme.colors.text;font.pixelSize:12;horizontalAlignment:Text.AlignHCenter;verticalAlignment:Text.AlignTop;topPadding:5}
                }
            }
        }
        Row {
            width:parent.width
            PanelLabel {width:parent.width-58;text:Qt.formatDateTime(root.selected,Qt.locale().dateFormat(root.compact?Locale.LongFormat:Locale.ShortFormat));font.pixelSize:root.compact?15:13;font.weight:Font.DemiBold;verticalAlignment:Text.AlignVCenter;height:24}
            AbstractButton {width:58;height:24;enabled:!root.busy;text:"Today";Accessible.name:"Go to today";contentItem:PanelLabel {text:parent.text;color:(root.monochrome?Theme.colors.muted:Theme.colors.accent);horizontalAlignment:Text.AlignRight;verticalAlignment:Text.AlignVCenter} onClicked:root.goToday()}
        }
        PanelLabel {visible:root.events.length===0;width:parent.width;wrapMode:Text.WordWrap;color:Theme.colors.muted;text:!root.loaded?(root.busy?"Loading calendar…":"Calendar unavailable"):!root.configured?"Connect a calendar to see your appointments here.":!root.lastSuccess?"Calendars are configured, but no successful sync has completed.":"No events on this day"}
        ScrollView {
            visible:root.events.length>0;width:parent.width;height:root.compact?eventRows.height:Math.min(220,eventRows.height);clip:true
            ScrollBar.horizontal.policy:ScrollBar.AlwaysOff
            Column {
                id:eventRows;width:content.width;spacing:4
                Repeater {
                    model:root.compact?root.events.slice(0,3):root.events
                    delegate:AbstractButton {
                        id:eventButton
                        required property var modelData
                        width:eventRows.width;height:eventText.implicitHeight+14;leftPadding:10;rightPadding:8;topPadding:5;bottomPadding:5;enabled:!root.busy
                        Accessible.name:modelData.title+", "+(modelData.allDay?"All day":modelData.startTime+" to "+modelData.endTime)
                        background:Rectangle {radius:6;color:parent.hovered || parent.activeFocus?Theme.colors.hover:Theme.colors.surface;Rectangle {width:3;anchors.top:parent.top;anchors.bottom:parent.bottom;anchors.margins:5;radius:2;color:root.tint(eventButton.modelData)}}
                        contentItem:Column {id:eventText;spacing:2
                            PanelLabel {width:eventButton.width-24;text:eventButton.modelData.title;textFormat:Text.PlainText;wrapMode:Text.WordWrap;maximumLineCount:2;elide:Text.ElideRight;font.weight:Font.Medium}
                            PanelLabel {width:eventButton.width-24;font.pixelSize:11;color:Theme.colors.muted;text:root.timeLabel(eventButton.modelData)+" · "+eventButton.modelData.calendar;textFormat:Text.PlainText;elide:Text.ElideRight}
                        }
                        onClicked:{if(root.compact){root.eventRequested(modelData,root.selected);return;}root.deletePending=false;root.detail=root.detail && root.detail.id===modelData.id?null:modelData;}
                    }
                }
            }
        }
        PanelLabel {visible:!root.compact && root.total>root.events.length;width:parent.width;wrapMode:Text.WordWrap;color:Theme.colors.muted;font.pixelSize:11;text:"Showing the first 100 events of "+root.total}
        Column {
            visible:!!root.detail;width:parent.width;spacing:4
            PanelLabel {width:parent.width;wrapMode:Text.WordWrap;text:root.detail?root.detail.title:"";textFormat:Text.PlainText;font.weight:Font.DemiBold}
            PanelLabel {width:parent.width;wrapMode:Text.WordWrap;text:root.detail?root.detail.provider+" · "+root.detail.calendar+(root.detail.location?" · "+root.detail.location:""):"";textFormat:Text.PlainText;color:Theme.colors.muted;font.pixelSize:11}
            ScrollView {visible:!!root.detail && !!root.detail.notes;width:parent.width;height:Math.min(90,noteText.implicitHeight);clip:true;ScrollBar.horizontal.policy:ScrollBar.AlwaysOff
                PanelLabel {id:noteText;width:content.width;wrapMode:Text.WordWrap;text:root.detail?root.detail.notes:"";textFormat:Text.PlainText;font.pixelSize:12}
            }
        }
        Row {
            visible:!!root.detail && root.detail.provider==="Google";width:parent.width;spacing:8
            DialogButton {width:(parent.width-8)/2;text:root.detail && root.detail.recurring?"Edit Occurrence…":"Edit…";enabled:!root.busy && !!root.detail && root.detail.editable;onClicked:root.startEdit(root.detail)}
            DialogButton {width:(parent.width-8)/2;text:root.deletePending?"Confirm Delete":"Delete…";enabled:!root.busy && !!root.detail && root.detail.editable;onClicked:{if(!root.deletePending){root.deletePending=true;return;}root.send({action:"delete",accountGeneration:root.detail.accountGeneration,calendarId:root.detail.calendarId,eventId:root.detail.eventId,recurrenceId:root.detail.recurrenceId,etag:root.detail.etag,confirmDelete:true});}}
        }
        PanelLabel {visible:!!root.detail && root.detail.provider==="Google" && (!root.detail.editable || root.deletePending || root.detail.recurring);width:parent.width;wrapMode:Text.WordWrap;color:Theme.colors.muted;font.pixelSize:11;text:root.deletePending?"Delete only this selected event/occurrence from Google?":root.detail && !root.detail.editable?"Some event details require Google Calendar; use Open Google below.":"Changes apply only to this occurrence."}
        DialogButton {visible:!root.compact && root.writableCalendars.length>0;width:parent.width;text:"New Event…";enabled:!root.busy;onClicked:root.startEdit(null)}
        PanelLabel {visible:!!root.mutationWarning;width:parent.width;wrapMode:Text.WordWrap;text:root.mutationWarning;color:Theme.colors.muted;font.pixelSize:11}
        DialogButton {visible:!root.compact && root.optionsOpen && root.configured;width:parent.width;text:root.busy && root.request.action==="sync"?"Refreshing calendars…":"Refresh Calendars";enabled:!root.busy;onClicked:root.send({action:"sync"})}
        Row {visible:!root.compact && root.optionsOpen;width:parent.width;spacing:8
            DialogButton {width:(parent.width-8)/2;text:"iCloud Accounts…";enabled:!root.busy;onClicked:root.accountsRequested()}
            DialogButton {width:(parent.width-8)/2;text:"Google Accounts…";enabled:!root.busy;onClicked:root.accountsRequested()}
        }
        Repeater {
            model:!root.compact && root.optionsOpen?root.providers.filter(provider=>provider.configured):[]
            delegate:Column {
                required property var modelData
                width:content.width;spacing:3
                PanelLabel {width:parent.width;wrapMode:Text.WordWrap;text:modelData.name+(modelData.lastSuccess?(modelData.id==="google"?" · Updated ":" · Synced ")+Qt.formatDateTime(new Date(modelData.lastSuccess),"MMM d, h:mm AP"):" · Waiting for first sync");color:Theme.colors.muted;font.pixelSize:10}
                DialogButton {width:parent.width;text:"Open "+modelData.name;onClicked:AppLaunch.run(["xdg-open",modelData.id==="icloud"?"https://www.icloud.com/calendar/":"https://calendar.google.com/"])}
            }
        }
        PanelLabel {visible:!!root.syncError || !!root.warning || !!root.backgroundError;width:parent.width;wrapMode:Text.WordWrap;text:root.syncError || root.warning || root.backgroundError;textFormat:Text.PlainText;color:Theme.colors.muted;font.pixelSize:11}
        PanelLabel {visible:!!root.error;width:parent.width;wrapMode:Text.WordWrap;text:root.error;textFormat:Text.PlainText;color:Theme.colors.muted;font.pixelSize:12}
        DialogButton {visible:!root.compact && !!root.error;width:parent.width;text:"Retry";enabled:!root.busy;onClicked:root.refresh()}
        Row {visible:!root.compact;width:parent.width;spacing:8
            DialogButton {width:32;text:"…";Accessible.name:"Calendar accounts and refresh";onClicked:root.optionsOpen=!root.optionsOpen}
        }
        PanelLabel {visible:!root.compact && root.optionsOpen;width:parent.width;text:"Google syncs in the background · iCloud read-only · "+root.calendarTimeZone;color:Theme.colors.muted;font.pixelSize:10;wrapMode:Text.WordWrap}
    }
    // Compact content stays read-only; navigation hands off to the Calendar app.
    ControlRow {objectName:"calendar-open-related";visible:root.compact;width:parent.width-36;x:18;y:content.y+content.height+4;height:30;text:root.events.length>3?"Show all "+root.total+" events in Calendar":"Open Calendar";onClicked:root.openCalendar()}
    component CalendarField: TextField {
        height:30;font.family:Theme.font;font.pixelSize:12;color:Theme.colors.text;placeholderTextColor:Theme.colors.muted
        selectionColor:(root.monochrome?Theme.colors.muted:Theme.colors.accent);selectedTextColor:Theme.colors.selectedText
        background:Rectangle {radius:6;color:Theme.colors.field;border.color:parent.activeFocus?(root.monochrome?Theme.colors.muted:Theme.colors.accent):Theme.hairline}
    }
    Column {
        id:editor;visible:root.editing;x:18;y:18;width:parent.width-36;spacing:8
        PanelLabel {text:root.editEvent?(root.editEvent.recurring?"Edit this occurrence":"Edit event"):"New event";font.weight:Font.DemiBold}
        PanelLabel {text:"Calendar";color:Theme.colors.muted;font.pixelSize:11}
        DesktopComboBox {id:calendarChoice;width:parent.width;model:root.writableCalendars;textRole:"name";enabled:!root.busy && !root.editEvent;Accessible.name:"Event calendar"}
        CalendarField {id:editTitle;width:parent.width;placeholderText:"Event title";enabled:!root.busy;Accessible.name:"Event title";selectByMouse:true}
        CalendarField {id:editLocation;width:parent.width;placeholderText:"Location";enabled:!root.busy;Accessible.name:"Event location";selectByMouse:true}
        SettingsToggle {id:allDay;width:parent.width;height:36;text:"All day";enabled:!root.busy;onClicked:{if(checked){editStart.text=editStart.text.slice(0,10);editEnd.text=editEnd.text.slice(0,10);}else{editStart.text=editStart.text.slice(0,10)+"T09:00";editEnd.text=editEnd.text.slice(0,10)+"T10:00";}}}
        PanelLabel {width:parent.width;wrapMode:Text.WordWrap;text:allDay.checked?"All-day dates include the final day.":"Times in "+root.calendarTimeZone;color:Theme.colors.muted;font.pixelSize:11}
        DateTimeField {id:editStart;width:parent.width;label:"Starts";allDay:allDay.checked;enabled:!root.busy}
        DateTimeField {id:editEnd;width:parent.width;label:"Ends";allDay:allDay.checked;enabled:!root.busy}
        ScrollView {width:parent.width;height:90;clip:true;TextArea {id:editNotes;font.family:Theme.font;font.pixelSize:12;color:Theme.colors.text;placeholderTextColor:Theme.colors.muted;selectionColor:(root.monochrome?Theme.colors.muted:Theme.colors.accent);selectedTextColor:Theme.colors.selectedText;background:Rectangle {radius:6;color:Theme.colors.field;border.color:parent.activeFocus?(root.monochrome?Theme.colors.muted:Theme.colors.accent):Theme.hairline} placeholderText:"Notes";enabled:!root.busy;wrapMode:TextEdit.Wrap;Accessible.name:"Event notes";selectByMouse:true}}
        PanelLabel {visible:!!root.error;width:parent.width;wrapMode:Text.WordWrap;text:root.error;textFormat:Text.PlainText;color:Theme.colors.muted;font.pixelSize:11}
        Row {width:parent.width;spacing:8
            DialogButton {width:(parent.width-8)/2;text:"Cancel";enabled:!root.busy;onClicked:{root.editing=false;root.error="";}}
            DialogButton {width:(parent.width-8)/2;text:root.busy?"Saving…":"Save";primary:true;enabled:!root.busy;onClicked:root.saveEvent()}
        }
    }
    Process {
        id:worker;stdinEnabled:true
        property bool received:false
        property bool success:false
        command:["lumen-calendar"]
        onStarted:{received=false;success=false;write(JSON.stringify(root.request)+"\n");}
        stdout:StdioCollector {onStreamFinished:{
            try {
                const data=JSON.parse(text);worker.received=true;
                if(data.error){root.error=data.error;return;}
                if(data.mutationSuccess){root.editing=false;root.deletePending=false;root.mutationWarning=data.mutationWarning || "";}if(data.month===undefined || data.month===root.iso(root.month)){root.apply(data);root.updateSelection();}worker.success=true;
            }catch(e){root.error="Unable to read the calendar";}
        }}
        onExited:(code,status)=>{
            if(!received || code!==0){if(!root.error)root.error="Calendar did not respond";}
            if(root.refreshPending){root.refreshPending=false;Qt.callLater(root.refresh);}else if(success && root.request.action==="list")Qt.callLater(root.refreshBackground);
        }
    }
    Process {
        id:background;stdinEnabled:true;command:["lumen-calendar"]
        property int epoch
        property string requestMonth;property string requestDate
        onStarted:write(JSON.stringify({action:"load",selectedDate:requestDate,month:requestMonth})+"\n")
        stdout:StdioCollector {onStreamFinished:{
            try {const data=JSON.parse(text);if(data.error){root.backgroundError=data.error;return;}
                if(background.epoch===root.mutationEpoch && background.requestMonth===root.iso(root.month) && !root.editing && !root.busy){root.apply(data);root.updateSelection();root.backgroundError="";}
            }catch(e){root.backgroundError="Calendar refresh is unavailable. Saved events are still displayed.";}
        }}
        onExited:if(requestMonth!==root.iso(root.month) && root.active)Qt.callLater(root.refreshBackground)
    }
    Timer {interval:60000;repeat:true;running:root.active && !root.editing;onTriggered:root.refresh()}
}

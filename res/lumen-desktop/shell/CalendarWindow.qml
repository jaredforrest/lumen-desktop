import QtQuick
import QtQuick.Controls
import QtQuick.Window
import QtQuick.Effects
import Quickshell
import "CalendarLogic.js" as CalendarLogic
import "ScrollGeometry.js" as ScrollGeometry

FloatingWindow {
    id: root
    required property var shell
    title:"Calendar";visible:false;implicitWidth:1060;implicitHeight:720
    minimumSize:Qt.size(480,320);color:"transparent";surfaceFormat.opaque:false
    readonly property color canvasColor:Theme.dark?"#202022":"#ffffff"
    property alias calendarState:calendar
    property string view:"Month"
    property bool sidebar:true
    property bool minimized:false
    property bool inspector:false
    property var hiddenCalendars:[]
    readonly property int sidebarWidth:sidebar?Math.min(205+(Theme.textRole("label")-12)*8,Math.floor(width*.4)):0
    readonly property bool compactToolbar:width<850*Theme.textScale
    readonly property int toolbarHeight:compactToolbar?92:52
    readonly property int eventRowHeight:18+(Theme.textRole("metadata")-11)
    readonly property int dayMarkerSize:25+(Theme.textRole("body")-13)
    FontMetrics {id:displayTextMetrics;font.family:Theme.font;font.pixelSize:Theme.textRole("display")}
    readonly property bool stackedNavigation:Theme.textScale>1 || main.width<420
    readonly property int navigationY:stackedNavigation?Math.max(45,Math.ceil(displayTextMetrics.height)+8):3
    readonly property int viewStart:stackedNavigation?navigationY+Math.max(28,Theme.textRole("label")+10)+15:54
    FontMetrics {id:timeTextMetrics;font.family:Theme.font;font.pixelSize:Theme.textRole("metadata")}
    readonly property int timeHourHeight:Math.max(52,Math.ceil(timeTextMetrics.height)*3+14)
    readonly property var hourLabels:Array.from({length:24},(_,hour)=>Qt.formatTime(new Date(2026,0,1,hour),Qt.locale().timeFormat(Locale.ShortFormat)))
    Item {id:hourLabelMeasures;visible:false
        Repeater {model:root.hourLabels;PanelLabel {required property string modelData;text:modelData;font.pixelSize:Theme.textRole("caption")}}
    }
    readonly property int timeGutterWidth:Math.max(46,Math.ceil(Math.max(...Array.from(hourLabelMeasures.children).map(item=>Number(item.implicitWidth)||0)))+7)
    FontMetrics {id:yearDayMetrics;font.family:Theme.font;font.pixelSize:Theme.textRole("caption")}
    FontMetrics {id:yearTitleMetrics;font.family:Theme.font;font.pixelSize:Theme.textRole("subheading")}
    readonly property int yearDayHeight:Math.ceil(yearDayMetrics.height)+4
    readonly property int yearTitleHeight:Math.ceil(yearTitleMetrics.height)+8
    readonly property color red:Theme.dark?"#ff453a":"#ff3b30"
    readonly property date selected:calendar.selected
    readonly property date month:calendar.month
    readonly property date weekStart:new Date(selected.getFullYear(),selected.getMonth(),selected.getDate()-((selected.getDay()-Qt.locale().firstDayOfWeek%7+7)%7))
    readonly property var calendars: {
        let found=[];
        for(const provider of calendar.providers)for(const item of provider.calendars || [])found.push({key:provider.name+" / "+item.name,name:item.name,provider:provider.name});
        for(const event of calendar.monthEvents)if(!found.some(c=>c.key===key(event)))found.push({key:key(event),name:event.calendar,provider:event.provider});
        return found;
    }
    function key(event){return event.provider+" / "+event.calendar;}
    function tint(event){return CalendarLogic.tint(event,Theme.dark);}

    function events(d){const source=d.getMonth()===root.month.getMonth()?calendar:d<root.month?previousMonth:nextMonth;return source.dayEvents(d).filter(e=>!hiddenCalendars.includes(key(e)) && (!search.text || (e.title+" "+e.location+" "+e.notes).toLowerCase().includes(search.text.toLowerCase())));}
    function layout(events,day) {
        const key=calendar.iso(day), minutes=t=>Number(t.slice(0,2))*60+Number(t.slice(3,5));
        let rows=events.map(e=>({event:e,lo:e.date<key?0:minutes(e.startTime),hi:e.endDate>key?1440:minutes(e.endTime),column:0,columns:1}));
        rows.sort((a,b)=>a.lo-b.lo || b.hi-a.hi);
        let group=[],ends=[],end=-1;
        const finish=()=>{for(const row of group)row.columns=ends.length;group=[];ends=[];end=-1;};
        for(const row of rows){row.hi=Math.max(row.lo+15,row.hi);if(row.lo>=end)finish();let slot=ends.findIndex(v=>v<=row.lo);if(slot<0)slot=ends.length;ends[slot]=row.hi;row.column=slot;group.push(row);end=Math.max(end,row.hi);}
        finish();return rows;
    }
    property int dayFocusRequest:0
    onViewChanged:dayFocusRequest++
    onVisibleChanged:dayFocusRequest++
    function chooseAndFocusDay(value){
        const day=new Date(value),key=calendar.iso(day),owner=content.Window.window,origin=owner?owner.activeFocusItem:null,originWasCell=!!(origin && origin.day!==undefined && origin.inMonth!==undefined),request=++dayFocusRequest;
        choose(day);
        Qt.callLater(()=>{
            if(request!==dayFocusRequest || !visible || view!=="Month" || owner!==content.Window.window || !owner || !owner.active || calendar.iso(selected)!==key)return;
            const cells=[];for(let i=0;i<monthDays.count;i++)cells.push(monthDays.itemAt(i));
            const active=owner.activeFocusItem;
            if(!originWasCell || (active!==origin && (cells.includes(origin) || (active!==calendarViewport && active!==content && active!==owner.contentItem && active!==null))))return;
            const cell=cells.find(item=>item && calendar.iso(item.day)===key);if(cell)cell.forceActiveFocus();
        });
    }
    function choose(d){calendar.selectDate(d);}
    function step(delta){choose(view==="Year"?new Date(selected.getFullYear()+delta,selected.getMonth(),selected.getDate()):view==="Month"?new Date(month.getFullYear(),month.getMonth()+delta,1):new Date(selected.getFullYear(),selected.getMonth(),selected.getDate()+delta*(view==="Week"?7:1)));}
    WindowGeometry {id:windowGeometry;ownerWindow:root}
    function fitScreen() {
        if(!visible || !screen || !Quickshell.env("SWAYSOCK"))return;
        windowGeometry.fitWorkspace(32,100);
    }

    onScreenChanged:Qt.callLater(root.fitScreen)
    Connections {target:root.screen;function onWidthChanged(){Qt.callLater(root.fitScreen);}function onHeightChanged(){Qt.callLater(root.fitScreen);}}
    function openAt(day,event){open();calendar.selectDate(day);if(event){calendar.detail=event;calendar.editing=false;inspector=true;}}
    function open(){shell.claimPopup("");if(minimized && Quickshell.env("SWAYSOCK")){Quickshell.execDetached(["swaymsg",'[app_id="org.quickshell" title="^Calendar$"] move workspace current, focus']);minimized=false;}visible=true;calendar.refresh();Qt.callLater(root.fitScreen);Qt.callLater(()=>{content.forceActiveFocus();if(content.Window.window)content.Window.window.requestActivate();});shell.focusOwnedWindow("Calendar");}
    function eventDetails(event,day){if(day)choose(day);calendar.editing=false;calendar.detail=event;inspector=true;}
    function create(d,hour){choose(d);calendar.startEdit(null);if(hour!==undefined){calendar.setStartHour(hour);}inspector=true;}
    component Tool: AbstractButton {
        id:button;property string label;implicitWidth:Math.max(30,caption.implicitWidth+20);implicitHeight:Math.max(28,caption.implicitHeight+10)
        Accessible.name:label
        background:Rectangle {radius:6;color:button.pressed?Theme.colors.field:button.hovered?Theme.colors.hover:"transparent"}
        contentItem:PanelLabel {id:caption;text:button.label;horizontalAlignment:Text.AlignHCenter;verticalAlignment:Text.AlignVCenter;font.pixelSize:Theme.textRole("label")}
    }
    Rectangle {id:windowMask;width:root.width;height:root.height;radius:Theme.nativeWindowFrame?0:12;layer.enabled:!Theme.nativeWindowFrame && GraphicsInfo.api !== GraphicsInfo.Software;visible:false}
    CalendarCard {id:previousMonth;followingToday:false;visible:false;today:root.shell.currentDate;month:new Date(root.month.getFullYear(),root.month.getMonth()-1,1);selected:month;active:root.visible && root.view==="Week" && root.weekStart<root.month;onMonthChanged:if(active)Qt.callLater(refresh)}
    CalendarCard {id:nextMonth;followingToday:false;visible:false;today:root.shell.currentDate;month:new Date(root.month.getFullYear(),root.month.getMonth()+1,1);selected:month;active:root.visible && root.view==="Week" && new Date(root.weekStart.getFullYear(),root.weekStart.getMonth(),root.weekStart.getDate()+6)>=month;onMonthChanged:if(active)Qt.callLater(refresh)}
    Item {
        id:content;anchors.fill:parent;focus:true
        layer.enabled:!Theme.nativeWindowFrame && GraphicsInfo.api !== GraphicsInfo.Software;layer.effect:MultiEffect {maskEnabled:true;maskSource:windowMask}
        Rectangle {anchors.fill:parent;color:root.canvasColor}
        Keys.onPressed:event=>{
            if(event.key===Qt.Key_Escape){root.inspector=false;calendar.editing=false;event.accepted=true;}
            else if(event.modifiers & Qt.ControlModifier){
                if(event.key===Qt.Key_T){root.choose(new Date());event.accepted=true;}
                else if(event.key>=Qt.Key_1 && event.key<=Qt.Key_4){root.view=["Day","Week","Month","Year"][event.key-Qt.Key_1];event.accepted=true;}
                else if(event.key===Qt.Key_N){root.create(root.selected);event.accepted=true;}
                else if(event.key===Qt.Key_R){calendar.send({action:"sync"});event.accepted=true;}
                else if(event.key===Qt.Key_F){search.forceActiveFocus();event.accepted=true;}
                else if(event.key===Qt.Key_W){root.visible=false;event.accepted=true;}
            }
        }
        Rectangle {width:root.sidebarWidth;height:parent.height;color:Theme.dark?"#2b2b2e":"#f0f0f2"}
        Rectangle {x:root.sidebarWidth;width:parent.width-x;height:root.toolbarHeight;color:Theme.dark?"#29292c":"#f7f7f8"}
        MouseArea {x:0;y:0;width:parent.width;height:root.toolbarHeight;onPressed:if(content.Window.window)content.Window.window.startSystemMove()}
        WindowControls {windowName:"Calendar";windowActive:content.Window.window?content.Window.window.active:false;onTriggered:action=>{const w=content.Window.window;if(action===0)root.visible=false;else if(w && action===1){if(Quickshell.env("SWAYSOCK")){root.minimized=true;Quickshell.execDetached(["swaymsg",'[app_id="org.quickshell" title="^Calendar$"] move scratchpad']);}else w.showMinimized();}else if(w){if(w.visibility===Window.FullScreen)w.showNormal();else w.showFullScreen();}}}
        Tool {x:94;y:12;label:"☷";Accessible.name:"Show or hide calendars";onClicked:root.sidebar=!root.sidebar}
        Tool {x:134;y:12;label:"+";Accessible.name:"New event";Accessible.description:calendar.busy?"Loading calendars":calendar.writableCalendars.length?"Create an event":"Connect an editable Google calendar to create events";enabled:calendar.writableCalendars.length>0 && !calendar.busy;onClicked:root.create(root.selected)}
        Row {
            id:viewTabs;x:root.compactToolbar?(parent.width-width)/2:root.sidebarWidth+(parent.width-root.sidebarWidth-width)/2;y:root.compactToolbar?52:12;spacing:1
            Repeater {model:["Day","Week","Month","Year"]
                delegate:Tool {required property string modelData;label:modelData;width:Math.max(60,implicitWidth);background:Rectangle {radius:6;color:root.view===parent.modelData?(Theme.dark?"#555559":"#dedee1"):parent.hovered?Theme.colors.hover:"transparent"}onClicked:root.view=modelData}
            }
        }
        TextField {id:search;width:root.width<900?100+(Theme.textRole("label")-12)*5:155;height:Math.max(28,Theme.textRole("label")+14);anchors.right:parent.right;anchors.rightMargin:16;y:12;placeholderText:"Search";font.family:Theme.font;font.pixelSize:Theme.textRole("label");color:Theme.colors.text;placeholderTextColor:Theme.colors.muted;selectByMouse:true;Accessible.name:"Search this month's events";background:Rectangle {radius:6;color:Theme.colors.field;border.color:parent.activeFocus?Theme.colors.accent:Theme.hairline}}
        ScrollView {id:calendarSidebarScroll;objectName:"calendar-sidebar-viewport";visible:root.sidebar;x:12;y:root.toolbarHeight+20;width:root.sidebarWidth-24;height:parent.height-y-45;clip:true
            FocusFlickable {id:calendarSidebarContent;clip:true;contentWidth:width;contentHeight:calendarSidebarColumn.height
            Column {id:calendarSidebarColumn;width:calendarSidebarScroll.availableWidth;spacing:6;onPositioningComplete:Qt.callLater(calendarSidebarContent.revealFocus)
                PanelLabel {text:"Calendars";font.pixelSize:Theme.textRole("metadata");font.weight:Font.DemiBold;color:Theme.colors.muted;bottomPadding:8}
                Repeater {model:root.calendars
                    delegate:AbstractButton {id:calButton;required property var modelData;width:root.sidebarWidth-24;height:31+(Theme.textRole("label")-12)
                        onClicked:{let keys=root.hiddenCalendars.slice();const i=keys.indexOf(modelData.key);if(i<0)keys.push(modelData.key);else keys.splice(i,1);root.hiddenCalendars=keys;}
                        Accessible.name:modelData.provider+" "+modelData.name;checkable:true;checked:!root.hiddenCalendars.includes(modelData.key)
                        background:Rectangle {radius:5;color:calButton.hovered?Theme.colors.hover:"transparent"}
                        contentItem:Item {Rectangle {id:calendarMark;x:5;y:8;width:13;height:13;radius:3;color:calButton.checked?root.tint({provider:calButton.modelData.provider,calendar:calButton.modelData.name}):"transparent";border.color:Theme.colors.muted;PanelLabel {anchors.centerIn:parent;text:calButton.checked?"✓":"";color:Theme.textOnOpaqueFill(calendarMark.color);font.pixelSize:10/* fixed mark symbol */}}PanelLabel {x:26;width:parent.width-29;anchors.verticalCenter:parent.verticalCenter;text:calButton.modelData.name;textFormat:Text.PlainText;elide:Text.ElideRight;font.pixelSize:Theme.textRole("label")}}
                    }
                }
                PanelLabel {visible:root.calendars.length===0;width:parent.width;wrapMode:Text.WordWrap;text:calendar.loaded?"Add an account to see your calendars.":"Loading saved calendars…";font.pixelSize:Theme.textRole("label");color:Theme.colors.muted}
                PanelLabel {visible:calendar.loaded && root.calendars.length>0 && calendar.writableCalendars.length===0;width:root.sidebarWidth-24;wrapMode:Text.WordWrap;text:"These calendars are read-only. Connect an editable Google calendar to create events.";color:Theme.colors.muted;font.pixelSize:Theme.textRole("metadata")}
                Tool {width:root.sidebarWidth-24;label:"Accounts…";onClicked:root.shell.openSettings("Internet Accounts")}
                Tool {width:root.sidebarWidth-24;label:calendar.refreshing?"Updating…":"Refresh Calendars";enabled:!calendar.busy && !calendar.refreshing;onClicked:calendar.send({action:"sync"})}
                PanelLabel {visible:!!calendar.syncError || !!calendar.error || !!calendar.backgroundError;width:parent.width;wrapMode:Text.WordWrap;font.pixelSize:Theme.textRole("metadata");color:Theme.colors.muted;text:calendar.error || calendar.syncError || calendar.backgroundError;textFormat:Text.PlainText}
            }
        }
        }
        ScrollView {
            id:calendarViewport;objectName:"calendar-content-viewport";x:root.sidebarWidth+20;y:root.toolbarHeight+12;width:Math.max(1,parent.width-x-20);height:Math.max(1,parent.height-y-16);clip:true
            contentWidth:main.width;contentHeight:main.height
            ScrollBar.horizontal:DesktopScrollBar {}
            ScrollBar.vertical:DesktopScrollBar {}
            function revealFocus(){
                if(!content.Window.window)return;
                const item=content.Window.window.activeFocusItem;if(!item)return;
                let ancestor=item;while(ancestor && ancestor!==main)ancestor=ancestor.parent;if(!ancestor)return;
                const at=item.mapToItem(main,0,0),flick=calendarViewport.contentItem;
                flick.contentX=ScrollGeometry.reveal(flick.contentX,availableWidth,main.width,at.x,item.width,0);
                flick.contentY=ScrollGeometry.reveal(flick.contentY,availableHeight,main.height,at.y,item.height,0);
            }
            Connections {target:content.Window.window;function onActiveFocusItemChanged(){Qt.callLater(calendarViewport.revealFocus);}}
            Connections {target:content.Window.window?content.Window.window.activeFocusItem:null;ignoreUnknownSignals:true
                function onXChanged(){Qt.callLater(calendarViewport.revealFocus);}
                function onYChanged(){Qt.callLater(calendarViewport.revealFocus);}
                function onWidthChanged(){Qt.callLater(calendarViewport.revealFocus);}
                function onHeightChanged(){Qt.callLater(calendarViewport.revealFocus);}
            }
            onWidthChanged:Qt.callLater(revealFocus)
            onHeightChanged:Qt.callLater(revealFocus)
            onContentWidthChanged:Qt.callLater(revealFocus)
            onContentHeightChanged:Qt.callLater(revealFocus)
            Item {
            id:main;width:Math.max(calendarViewport.availableWidth,(root.view==="Day"?160:420)*Theme.textScale);height:Math.max(calendarViewport.availableHeight,root.view==="Year"?root.viewStart+4*(root.yearTitleHeight+6*root.yearDayHeight)+36:400*Theme.textScale)
            PanelLabel {width:parent.width;text:root.view==="Year"?root.selected.getFullYear():Qt.formatDateTime(root.month,"MMMM yyyy");font.pixelSize:Theme.textRole("display");font.weight:Font.Light}
            Row {anchors.right:parent.right;y:root.navigationY;spacing:3;Tool {label:"‹";Accessible.name:"Previous "+root.view.toLowerCase();onClicked:root.step(-1)}Tool {label:"Today";onClicked:root.choose(new Date())}Tool {label:"›";Accessible.name:"Next "+root.view.toLowerCase();onClicked:root.step(1)}}
            Column {visible:root.view==="Month";y:root.viewStart;width:parent.width;height:parent.height-y;spacing:0
                Row {width:parent.width;height:25;Repeater {model:CalendarLogic.weekdayNames(Qt.locale(),Qt.locale().firstDayOfWeek,Locale.ShortFormat);PanelLabel {required property string modelData;width:main.width/7;height:25;text:modelData;horizontalAlignment:Text.AlignRight;rightPadding:9;font.pixelSize:Theme.textRole("metadata");color:Theme.colors.muted}}}
                Grid {width:parent.width;height:parent.height-25;columns:7
                    Repeater {id:monthDays;model:7*calendar.weekCount
                        delegate:Rectangle {id:cell;required property int index;width:main.width/7;height:(parent.height)/calendar.weekCount
                            readonly property date day:new Date(root.month.getFullYear(),root.month.getMonth(),index-CalendarLogic.offset(root.month,Qt.locale().firstDayOfWeek)+1)
                            readonly property bool current:calendar.iso(day)===calendar.iso(calendar.today)
                            readonly property bool inMonth:day.getMonth()===root.month.getMonth()
                            readonly property var items:inMonth?root.events(day):[]
                            activeFocusOnTab:true;Accessible.role:Accessible.Button;Accessible.name:Qt.formatDateTime(day,"dddd d MMMM yyyy");Accessible.selected:calendar.iso(day)===calendar.iso(root.selected)
                            Keys.onPressed:event=>{let offset=0;if(event.key===Qt.Key_Left)offset=-1;else if(event.key===Qt.Key_Right)offset=1;else if(event.key===Qt.Key_Up)offset=-7;else if(event.key===Qt.Key_Down)offset=7;else if(event.key===Qt.Key_Return || event.key===Qt.Key_Space){root.chooseAndFocusDay(day);event.accepted=true;return;}else {event.accepted=false;return;}const next=new Date(day);next.setDate(next.getDate()+offset);root.chooseAndFocusDay(next);event.accepted=true;}
                            color:calendar.iso(day)===calendar.iso(root.selected)?(Theme.dark?"#323235":"#f5f5f7"):(index%7>4?(Theme.dark?"#242426":"#fafafa"):root.canvasColor)
                            border.width:activeFocus?2:0.5;border.color:activeFocus?Theme.colors.accent:Theme.hairline
                            MouseArea {anchors.fill:parent;onClicked:root.choose(cell.day);onDoubleClicked:if(calendar.writableCalendars.length)root.create(cell.day)}
                            Rectangle {anchors.right:parent.right;anchors.rightMargin:5;y:4;width:root.dayMarkerSize;height:root.dayMarkerSize;radius:width/2;color:cell.current?root.red:"transparent";PanelLabel {anchors.centerIn:parent;text:cell.day.getDate();font.pixelSize:Theme.textRole("body");color:cell.current?Theme.textOnOpaqueFill(root.red):cell.inMonth?Theme.colors.text:Theme.colors.muted}}
                            Column {x:3;y:root.dayMarkerSize+8;width:parent.width-6;spacing:2
                                Repeater {model:cell.items.slice(0,Math.max(0,Math.floor((cell.height-root.dayMarkerSize-27)/(root.eventRowHeight+1))))
                                    delegate:AbstractButton {id:eventButton;required property var modelData;width:parent.width;height:root.eventRowHeight;Accessible.name:modelData.title
                                        background:Rectangle {radius:3;color:eventButton.modelData.allDay?Qt.alpha(root.tint(eventButton.modelData),Theme.dark?.35:.18):"transparent"}
                                        contentItem:PanelLabel {text:(eventButton.modelData.allDay?"":eventButton.modelData.startTime+" ")+eventButton.modelData.title;textFormat:Text.PlainText;elide:Text.ElideRight;font.pixelSize:Theme.textRole("metadata");color:root.tint(eventButton.modelData);leftPadding:4}
                                        onClicked:root.eventDetails(modelData,cell.day)
                                    }
                                }
                                PanelLabel {visible:cell.items.length>Math.max(0,Math.floor((cell.height-root.dayMarkerSize-27)/(root.eventRowHeight+1)));text:"+"+(cell.items.length-Math.max(0,Math.floor((cell.height-root.dayMarkerSize-27)/(root.eventRowHeight+1))))+" more";font.pixelSize:Theme.textRole("caption");color:Theme.colors.muted}
                            }
                        }
                    }
                }
            }
            Grid {visible:root.view==="Year";y:root.viewStart;width:parent.width;height:parent.height-y;columns:3;rowSpacing:12;columnSpacing:20
                Repeater {model:12
                    delegate:Item {id:mini;required property int index;width:(main.width-40)/3;height:Math.max((parent.height-36)/4,root.yearTitleHeight+6*root.yearDayHeight)
                        readonly property date first:new Date(root.selected.getFullYear(),index,1)
                        PanelLabel {text:Qt.formatDateTime(mini.first,"MMMM");color:Theme.errorText;font.pixelSize:Theme.textRole("subheading");font.weight:Font.Medium}
                        Grid {y:root.yearTitleHeight;width:parent.width;columns:7;onPositioningComplete:Qt.callLater(calendarViewport.revealFocus)
                            Repeater {model:42;delegate:AbstractButton {required property int index;width:mini.width/7;height:(mini.height-root.yearTitleHeight)/6
                                readonly property int number:index-CalendarLogic.offset(mini.first,Qt.locale().firstDayOfWeek)+1
                                readonly property bool valid:number>0 && number<=new Date(mini.first.getFullYear(),mini.index+1,0).getDate()
                                enabled:valid
                                contentItem:PanelLabel {text:parent.valid?parent.number:"";horizontalAlignment:Text.AlignHCenter;font.pixelSize:Theme.textRole("caption");color:calendar.iso(new Date(mini.first.getFullYear(),mini.index,parent.number))===calendar.iso(calendar.today)?Theme.errorText:Theme.colors.text}
                                onClicked:{root.choose(new Date(mini.first.getFullYear(),mini.index,number));root.view="Month";}
                            }}
                        }
                    }
                }
            }
            Column {visible:root.view==="Day" || root.view==="Week";y:root.viewStart;width:parent.width;height:parent.height-y;spacing:0
                readonly property int count:root.view==="Week"?7:1
                id:timeView
                Row {x:root.timeGutterWidth;height:36;Repeater {model:timeView.count;Tool {required property int index;width:(main.width-root.timeGutterWidth)/timeView.count
                    readonly property date day:root.view==="Week"?new Date(root.weekStart.getFullYear(),root.weekStart.getMonth(),root.weekStart.getDate()+index):root.selected
                    label:Qt.formatDateTime(day,"ddd d");onClicked:root.choose(day)
                }}}
                Row {id:allDayBand;objectName:"calendar-all-day-band";x:root.timeGutterWidth;height:Math.max(64,...Array.from(children).map(item=>Number(item.implicitHeight) || 0))
                    Repeater {model:timeView.count
                        Column {id:allDays;required property int index;width:(main.width-root.timeGutterWidth)/timeView.count;spacing:2
                            readonly property date day:root.view==="Week"?new Date(root.weekStart.getFullYear(),root.weekStart.getMonth(),root.weekStart.getDate()+index):root.selected
                            Repeater {model:root.events(allDays.day).filter(e=>e.allDay).slice(0,3)
                                Tool {required property var modelData;width:allDays.width-3;height:Math.max(19,implicitHeight);label:modelData.title;background:Rectangle {radius:3;color:Qt.alpha(root.tint(parent.modelData),.22)}onClicked:root.eventDetails(modelData,allDays.day)}
                            }
                        }
                    }
                }
                ScrollView {id:timeScroll;objectName:"calendar-time-scroll";width:parent.width;height:Math.max(0,parent.height-36-allDayBand.height);clip:true
                    Flickable {id:timeline;contentWidth:timeView.width;contentHeight:24*root.timeHourHeight;contentY:7*root.timeHourHeight;boundsBehavior:Flickable.StopAtBounds
                        Repeater {model:24
                            Item {required property int index;y:index*root.timeHourHeight;width:timeline.width;height:root.timeHourHeight
                                PanelLabel {objectName:"calendar-hour-label";width:root.timeGutterWidth-7;text:root.hourLabels[index];font.pixelSize:Theme.textRole("caption");color:Theme.colors.muted;horizontalAlignment:Text.AlignRight}
                                Rectangle {x:root.timeGutterWidth;width:parent.width-root.timeGutterWidth;height:1;color:Theme.hairline}
                            }
                        }
                        Row {x:root.timeGutterWidth;width:timeline.width-root.timeGutterWidth;height:timeline.contentHeight
                            Repeater {model:timeView.count
                                Item {id:dayColumn;required property int index;width:(timeline.width-root.timeGutterWidth)/timeView.count;height:timeline.contentHeight
                                    readonly property date day:root.view==="Week"?new Date(root.weekStart.getFullYear(),root.weekStart.getMonth(),root.weekStart.getDate()+index):root.selected
                                    readonly property var appointments:root.layout(root.events(day).filter(e=>!e.allDay),day)
                                    Rectangle {width:.5;height:parent.height;color:Theme.hairline}
                                    MouseArea {anchors.fill:parent;onDoubleClicked:mouse=>{if(calendar.writableCalendars.length)root.create(dayColumn.day,Math.min(23,Math.floor(mouse.y/root.timeHourHeight)));}}
                                    Repeater {model:dayColumn.appointments
                                        AbstractButton {id:appointment;required property var modelData
                                            x:modelData.column*dayColumn.width/modelData.columns+2;y:modelData.lo/60*root.timeHourHeight
                                            width:dayColumn.width/modelData.columns-4;height:Math.max(Math.ceil(timeTextMetrics.height)+2,(modelData.hi-modelData.lo)/60*root.timeHourHeight-1)
                                            Accessible.name:modelData.event.title;Accessible.description:modelData.event.startTime+"–"+modelData.event.endTime
                                            background:Rectangle {radius:4;color:Qt.alpha(root.tint(appointment.modelData.event),Theme.dark?.3:.16);Rectangle {width:3;height:parent.height;radius:2;color:root.tint(appointment.modelData.event)}}
                                            contentItem:PanelLabel {padding:Math.max(1,Math.min(5,(appointment.height-timeTextMetrics.height)/2));maximumLineCount:Math.max(1,Math.floor((appointment.height-2*padding)/timeTextMetrics.height));elide:Text.ElideRight;text:appointment.modelData.event.title+"\n"+appointment.modelData.event.startTime;textFormat:Text.PlainText;wrapMode:Text.WordWrap;clip:true;font.pixelSize:Theme.textRole("metadata");color:root.tint(appointment.modelData.event)}
                                            onClicked:root.eventDetails(modelData.event,dayColumn.day)
                                        }
                                    }
                                    Rectangle {visible:calendar.iso(dayColumn.day)===calendar.iso(calendar.today);y:(calendar.today.getHours()+calendar.today.getMinutes()/60)*root.timeHourHeight;width:parent.width;height:1;color:root.red}
                                }
                            }
                        }
                    }
                }
            }
        }
        }
        Rectangle {visible:root.inspector;anchors.fill:parent;color:"#30000000";MouseArea {anchors.fill:parent;onClicked:{root.inspector=false;calendar.editing=false;}}}
        ScrollView {id:inspectorScroll;visible:root.inspector;anchors.right:parent.right;anchors.rightMargin:18;y:root.toolbarHeight+6;width:Math.min(350*Theme.textScale,parent.width-36);height:Math.min(parent.height-y-18,calendar.implicitHeight);clip:true
            CalendarCard {id:calendar;width:inspectorScroll.availableWidth;today:root.shell.currentDate;detailsOnly:true;active:root.visible;onOpenCalendar:root.inspector=false;onAccountsRequested:root.shell.openSettings("Internet Accounts")}
        }
    }
}

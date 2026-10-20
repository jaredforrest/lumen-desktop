import QtQuick
import QtQuick.Controls
import Quickshell
import Quickshell.Bluetooth
import "NetworkState.js" as NetworkState
Item {
    id: root
    required property string kind
    required property var shell
    required property date today
    property real notificationHeight:420
    property string editingFocus: ""
    readonly property var internet:NetworkState.internet(root.shell.systemState && root.shell.systemState.internet)
    readonly property var focusState:root.shell.notifications.focusState || ({profiles:[],active:"",name:"",until:0,scheduled:false})
    function dateStatus(){return datePanel.status();}
    signal settingsRequested()
    signal panelRequested(string name)
    property real maximumWidth:344
    width: kind==="Date" ? Math.min(344,maximumWidth) : Math.min(300,maximumWidth)
    readonly property bool enclosingMaterial:["Controls","Date","Now Playing","VPN"].includes(kind)
    implicitHeight:panelContents.height+(enclosingMaterial?24:0)
    height:implicitHeight
    PanelSurface {anchors.fill:parent;visible:root.enclosingMaterial}
    Column {id:panelContents;x:root.enclosingMaterial?12:0;y:x;width:parent.width-x*2;spacing:12
    ControlCentre {visible:root.kind==="Controls";width:parent.width;shell:root.shell;onSettingsRequested:root.settingsRequested();onPanelRequested:name=>root.panelRequested(name)}
    PanelSurface {visible:root.kind==="Internet";width:parent.width;height:internetBody.height+28
      Column {id:internetBody;x:14;y:14;width:parent.width-28;spacing:8
        ControlRow {width:parent.width;text:"‹  Control Centre";onClicked:root.panelRequested("Controls")}
        PanelLabel {text:"Internet";font.weight:Font.DemiBold;font.pixelSize:15}
        PanelLabel {width:parent.width;wrapMode:Text.WordWrap;elide:Text.ElideNone;
          color:root.internet.status==="online"?Theme.colors.text:root.internet.status==="offline"?Theme.errorText:Theme.warningText;
          text:root.internet.message || "Checking internet connection…"}
        PanelLabel {width:parent.width;wrapMode:Text.WordWrap;elide:Text.ElideNone;color:Theme.colors.muted;
          text:{const s=root.internet,r=s.routes || ({}),v4=r["4"] || ({}),v6=r["6"] || ({});
            return "IPv4: "+(v4.available?(v4.interface || "route found"):"no route")+" · IPv6: "+(v6.available?(v6.interface || "route found"):"no route");}}
        PanelLabel {width:parent.width;wrapMode:Text.WordWrap;elide:Text.ElideNone;color:Theme.colors.muted;
          text:{const s=root.internet,d=s.dns || ({}),v=s.vpn || ({});
            return "DNS: "+(d.status || "not checked")+" · Mullvad: "+(v.state || "unavailable")+(v.selected_route?" (selected route)":"")+(v.locked_down?" (lockdown on)":"");}}
        PanelLabel {width:parent.width;wrapMode:Text.WordWrap;elide:Text.ElideNone;color:Theme.colors.muted;
          text:{const s=root.internet,c=s.checks || [],ok=c.filter(x=>x.ok).length;
            return c.length?"Secure reachability: "+ok+" of "+c.length+" checks passed":"Secure reachability: no route to test";}}
        ControlRow {width:parent.width;text:"Check again";onClicked:root.shell.systemState.refreshInternet(true)}
        ControlRow {width:parent.width;text:"Network settings…";onClicked:root.shell.openSettings("Network")}
        ControlRow {width:parent.width;text:"VPN controls…";onClicked:root.panelRequested("VPN")}
      }
    }
    PanelSurface {visible:root.kind==="Focus" && root.editingFocus==="";width:parent.width;height:focusBody.height+32
      Column {id:focusBody;x:16;y:16;width:parent.width-32;spacing:8
        ControlRow {width:parent.width;text:"‹  Control Centre";onClicked:root.panelRequested("Controls")}
        PanelLabel {text:"Focus";font.weight:Font.DemiBold}
        PanelLabel {width:parent.width;wrapMode:Text.WordWrap;text:root.focusState.active?(root.focusState.name+(root.focusState.until?" · until "+Qt.formatTime(new Date(root.focusState.until*1000),"HH:mm"):root.focusState.scheduled?" · scheduled":" · until turned off")):"Off";color:Theme.colors.muted}
        ControlRow {width:parent.width;text:"Off";onClicked:root.shell.notifications.command({action:"focus",profile:""})}
        Repeater {model:root.focusState.profiles
            delegate:Column {required property var modelData;width:parent.width;spacing:2
                ControlRow {width:parent.width;text:(root.focusState.active===modelData.id?"✓  ":"")+modelData.name;onClicked:root.shell.notifications.command({action:"focus",profile:modelData.id})}
                ControlRow {width:parent.width;text:"For one hour";onClicked:root.shell.notifications.command({action:"focus",profile:modelData.id,seconds:3600})}
                ControlRow {width:parent.width;text:"Configure…";onClicked:root.editingFocus=modelData.id}
            }
        }
        ControlRow {width:parent.width;text:"Use schedules";onClicked:root.shell.notifications.command({action:"focus",profile:null})}
        PanelLabel {width:parent.width;wrapMode:Text.WordWrap;color:Theme.colors.muted;text:"Focus silences banners except allowed apps. Notifications remain in Notification Centre."}
    }
      }
    PanelSurface {visible:root.kind==="Focus" && root.editingFocus!=="";width:parent.width;height:focusEditorBody.height+32
      Column {id:focusEditorBody;x:16;y:16;width:parent.width-32;spacing:8
        ControlRow {width:parent.width;text:"‹  Focus";onClicked:root.editingFocus=""}
        FocusProfileEditor {width:parent.width;service:root.shell.notifications;profileId:root.editingFocus}
    }
      }
    PanelLabel {visible:root.kind==="Controls" && text!=="";width:parent.width;wrapMode:Text.WordWrap;text:root.shell.systemState.error;color:Theme.colors.muted}
    PanelSurface {
        visible:root.kind==="Sound"
        width:parent.width; height:body.height+32
        Column {
            id:body;x:16;y:16;width:parent.width-32;spacing:12
            SoundControl { shell:root.shell;width:parent.width }
            Rectangle { width:parent.width;height:1;color:Theme.hairline }
            PanelLabel { text:"Output";font.weight:Font.DemiBold }
            Repeater {
                model: root.shell.audioState.outputs
                delegate: ControlRow {
                    required property var modelData
                    width: body.width; height: 36
                    text: (root.shell.audioState.sink === modelData ? "✓  " : "    ") + (modelData.nickname || modelData.description || modelData.name)
                    Accessible.name: "Use " + (modelData.description || modelData.name)
                    onClicked:root.shell.audioState.selectOutput(modelData)
                }
            }
            PanelLabel {visible:root.shell.audioState.outputs.length===0;text:"No output devices";color:Theme.colors.muted}
            ControlRow { width:parent.width;text:root.shell.audioState.muted?"Unmute":"Mute";enabled:root.shell.audioState.available;onClicked:root.shell.audioState.toggleMute() }

            ControlRow {width:parent.width;height:25;text:"Sound Settings…";onClicked:root.settingsRequested()}
        }
    }
    CaptureReport {visible:root.kind==="Capture";width:parent.width;state:root.shell.capture || null}
    NotificationPanel {id:datePanel;visible:root.kind==="Date";width:parent.width;shell:root.shell;today:root.today}
    ConnectivityPanel {visible:root.kind==="Wi-Fi" || root.kind==="Bluetooth";kind:root.kind;controls:root.shell.systemState;onBackRequested:root.panelRequested("Controls")}
    VpnPanel {visible:root.kind==="VPN";width:parent.width;compact:true;service:root.shell.vpn;onBackRequested:root.panelRequested("Controls");onSettingsRequested:root.shell.openSettings("VPN")}
    CalendarCard {
        visible:root.kind==="Calendar"
        width:parent.width;today:root.today;compact:true;onOpenCalendar:day=>root.shell.openCalendarAt(day,null);onEventRequested:(event,day)=>root.shell.openCalendarAt(day,event)
    }

    Column {visible:root.kind==="Now Playing";width:parent.width;spacing:8
        ControlRow {width:parent.width;text:"‹  Control Centre";onClicked:root.panelRequested("Controls")}
        MediaControl {width:parent.width;height:Math.max(245,implicitHeight);expanded:true}
    }
    }
}

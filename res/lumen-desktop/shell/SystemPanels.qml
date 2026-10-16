import QtQuick
import QtQuick.Controls
import Quickshell
import Quickshell.Bluetooth
Column {
    id: root
    required property string kind
    required property var shell
    required property date today
    property real notificationHeight:420
    function dateStatus(){return datePanel.status();}
    signal settingsRequested()
    signal panelRequested(string name)
    property real maximumWidth:344
    width: kind==="Date" ? Math.min(344,maximumWidth) : Math.min(300,maximumWidth)
    spacing:12
    Item {
        visible:root.kind==="Controls";width:parent.width;height:450
        Column {width:145;spacing:10
            ControlTile {width:145;height:60;label:"Wi-Fi";symbol:"wifi";unavailable:!root.shell.systemState.wifi.available;active:root.shell.systemState.wifi.state==="COMPLETED";subtitle:active?"Connected":"Disconnected";onClicked:root.panelRequested("Wi-Fi")}
            ControlTile {width:145;height:60;label:"Bluetooth";symbol:"bluetooth";unavailable:!Bluetooth.defaultAdapter;active:!!Bluetooth.defaultAdapter && Bluetooth.defaultAdapter.enabled;subtitle:active?"On":"Off";onClicked:root.panelRequested("Bluetooth")}
            ControlTile {width:145;height:60;label:"Sharing";subtitle:"LocalSend";symbol:"share";unavailable:false;accessibleLabel:"Open LocalSend nearby sharing";onClicked:{root.shell.claimPopup("");AppLaunch.run(["localsend_app"]);}}
        }
        MediaControl {x:155;width:145;height:130;onExpandRequested:root.panelRequested("Now Playing")}
        ControlTile {x:155;y:140;width:145;height:60;label:"Displays";subtitle:"Settings";symbol:"screen";unavailable:false;onClicked:root.shell.openSettings("Displays")}
        ControlTile {y:210;width:145;height:60;label:"Appearance";subtitle:"Settings";symbol:"contrast";unavailable:false;onClicked:root.shell.openSettings("Appearance")}
        ControlTile {x:155;y:210;width:145;height:60;label:"Focus";toggle:true;symbol:"moon";unavailable:!root.shell.notifications.available;active:root.shell.notifications.dnd;subtitle:active?"Do Not Disturb":"Off";onClicked:root.shell.notifications.command({action:"dnd"})}
        PanelSurface {y:280;width:300;height:62;radius:24
            BrightnessControl {x:16;y:12;width:parent.width-32}
        }
        PanelSurface {y:352;width:300;height:62;radius:24
            SoundControl {x:16;y:12;width:parent.width-32;shell:root.shell;compact:true}
        }
        DialogButton {y:424;width:160;height:26;anchors.horizontalCenter:parent.horizontalCenter;text:"System Settings…";onClicked:root.settingsRequested()}
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
    NotificationPanel {id:datePanel;visible:root.kind==="Date";width:parent.width;shell:root.shell;today:root.today}
    ConnectivityPanel {visible:root.kind==="Wi-Fi" || root.kind==="Bluetooth";kind:root.kind;controls:root.shell.systemState;onBackRequested:root.panelRequested("Controls")}
    CalendarCard {
        visible:root.kind==="Calendar"
        width:parent.width;today:root.today;compact:true;onOpenCalendar:root.shell.openCalendarAt(selected,null);onEventRequested:(event,day)=>root.shell.openCalendarAt(day,event)
    }

    Column {visible:root.kind==="Now Playing";width:300;spacing:8
        ControlRow {width:parent.width;text:"‹  Control Centre";onClicked:root.panelRequested("Controls")}
        MediaControl {width:parent.width;height:245;expanded:true}
    }
}

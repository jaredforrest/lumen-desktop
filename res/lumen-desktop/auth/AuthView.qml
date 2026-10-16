import QtQuick
import QtQuick.Controls
import QtQuick.Effects
import Quickshell

Item {
    id: view
    focus: true
    property string displayName: Quickshell.env("LUMEN_AUTH_NAME") || "Jared"
    property string errorMessage: ""
    property string prompt: "Enter Password"
    property bool busy: false
    onBusyChanged: if (!busy) Qt.callLater(() => password.forceActiveFocus())
    property bool echoResponse: false
    property bool showSessionOptions: false
    property string selectedSession: "Sway"
    property date now: new Date()
    readonly property real unit: Math.max(0.65, Math.min(1.6, height / 900))
    signal submitted(string response)
    signal powerRequested(string action)
    signal sessionSelected(string session)
    Timer { interval: 1000; running: true; repeat: true; onTriggered: view.now = new Date() }
    function openPowerMenu() { powerPopup.open(); }
    component PowerIcon: Canvas {
        antialiasing: true
        onPaint: {
            const c = getContext("2d");
            c.reset(); c.scale(width / 24, height / 24);
            c.strokeStyle = "#eeeeee"; c.lineWidth = 1.8; c.lineCap = "round";
            c.beginPath(); c.arc(12, 13, 7.5, -Math.PI / 3, Math.PI * 4 / 3); c.stroke();
            c.beginPath(); c.moveTo(12, 3); c.lineTo(12, 12); c.stroke();
        }
    }
    function clear() { password.clear(); password.forceActiveFocus(); }
    function submit() {
        if (view.busy || !password.text.length) return;
        const response = password.text;
        password.clear();
        view.submitted(response);
    }
    Image { anchors.fill: parent; source: Qt.resolvedUrl("wallpaper.png"); fillMode: Image.PreserveAspectCrop; asynchronous: false }
    // Keep foreground contrast consistent on both the greeter and lock surfaces.
    Rectangle { anchors.fill: parent; color: "#08000000" }
    Column {
        anchors.horizontalCenter: parent.horizontalCenter
        y: Math.round(view.height * 0.092)
        spacing: -6 * view.unit
        Text {
            anchors.horizontalCenter: parent.horizontalCenter
            text: Qt.formatDate(view.now, Qt.locale().dateFormat(Locale.LongFormat).replace("dddd", "ddd"))
            font.family: "Interlude Variable"; font.pixelSize: 24 * view.unit; font.weight: Font.DemiBold
            color: "#d8e7ee"
        }
        Text {
            anchors.horizontalCenter: parent.horizontalCenter
            text: Qt.formatTime(view.now, Qt.locale().timeFormat(Locale.ShortFormat))
            font.family: "Interlude Variable"; font.pixelSize: Math.round(122 * view.unit); font.weight: Font.Medium
            color: "#c7e1e9"
            renderType: Text.CurveRendering
        }
    }
    Row {
        anchors.right: parent.right; anchors.top: parent.top
        anchors.margins: 16 * view.unit
        spacing: 18 * view.unit
        Text {
            anchors.verticalCenter: parent.verticalCenter
            text: Quickshell.env("LUMEN_AUTH_LAYOUT") || "UK"; color: "#eeeeee"
            font.family: "Interlude Variable"; font.pixelSize: 13 * view.unit
        }
        Button {
            width: 22 * view.unit; height: width
            contentItem: PowerIcon {}
            background: Item {}
            Accessible.name: "Power options"
            onClicked: powerPopup.open()
        }
    }
    Column {
        id: profile
        width: 300 * view.unit
        anchors.horizontalCenter: parent.horizontalCenter
        y: Math.min(view.height * 0.735, view.height - height - 35 * view.unit)
        spacing: 12 * view.unit
        Rectangle {
            anchors.horizontalCenter: parent.horizontalCenter
            width: 58 * view.unit; height: width; radius: width / 2
            color: "#89b3c7"; border.color: "#70ffffff"; border.width: 1
            Image {id:portrait;anchors.fill:parent;source:Quickshell.env("LUMEN_AUTH_PORTRAIT") || "";fillMode:Image.PreserveAspectCrop;visible:false}
            Rectangle {id:portraitMask;anchors.fill:parent;radius:width/2;color:"white";layer.enabled:true;visible:false}
            MultiEffect {anchors.fill:parent;source:portrait;maskEnabled:true;maskSource:portraitMask;visible:portrait.status===Image.Ready}
            Text {visible:portrait.status!==Image.Ready; anchors.centerIn: parent; text: view.displayName.charAt(0); color: "white"; font.family: "Interlude Variable"; font.pixelSize: 29 * view.unit; font.weight: Font.Medium }
        }
        Text {
            anchors.horizontalCenter: parent.horizontalCenter
            text: view.displayName; color: "white"
            font.family: "Interlude Variable"; font.pixelSize: 18 * view.unit; font.weight: Font.DemiBold
        }
        Rectangle {
            anchors.horizontalCenter: parent.horizontalCenter
            width: 230 * view.unit; height: 36 * view.unit; radius: height / 2
            color: "#35ffffff"; border.color: password.activeFocus ? "#b0ffffff" : "#65ffffff"; border.width: 1
            TextField {
                id: password
                focus: true
                objectName: "password"
                anchors.fill: parent; anchors.rightMargin: 35 * view.unit
                leftPadding: 13 * view.unit; rightPadding: 2 * view.unit
                font.family: "Interlude Variable"; font.pixelSize: 14 * view.unit
                color: "white"; placeholderTextColor: "#e5e5e5"
                placeholderText: view.busy ? "Please wait…" : view.prompt
                echoMode: view.echoResponse ? TextInput.Normal : TextInput.Password
                passwordCharacter: "●"; selectByMouse: true
                enabled: !view.busy; background: Item {}
                onAccepted: view.submit()
                Keys.onEscapePressed: password.clear()
                Component.onCompleted: forceActiveFocus()
            }
            Button {
                anchors.right: parent.right; anchors.verticalCenter: parent.verticalCenter
                anchors.rightMargin: 5 * view.unit
                width: 26 * view.unit; height: width
                enabled: !view.busy && password.text.length > 0
                text: "→"; font.pixelSize: 19 * view.unit
                contentItem: Text { text: parent.text; color: "white"; horizontalAlignment: Text.AlignHCenter; verticalAlignment: Text.AlignVCenter; font: parent.font }
                background: Rectangle { radius: width/2; color: parent.hovered ? "#45ffffff" : "#25ffffff" }
                onClicked: view.submit()
                Accessible.name: "Continue"
            }
        }
        Text {
            anchors.horizontalCenter: parent.horizontalCenter
            width: parent.width; horizontalAlignment: Text.AlignHCenter; wrapMode: Text.WordWrap
            text: view.errorMessage; color: "#fff1e9"
            font.family: "Interlude Variable"; font.pixelSize: 13 * view.unit
        }
    }
    Popup {
        id: powerPopup
        x: view.width - width - 12 * view.unit
        y: 48 * view.unit
        width: 210 * view.unit; padding: 6 * view.unit
        modal: false; focus: true
        closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside
        background: Rectangle {
            radius: 12 * view.unit; color: "#ed28313b"
            border.color: "#45ffffff"; border.width: 1
        }
        contentItem: Column {
            spacing: 2 * view.unit
            Repeater {
                model: [{name: "Sleep", action: "suspend"}, {name: "Restart", action: "reboot"}, {name: "Shut Down", action: "poweroff"}]
                Button {
                    required property var modelData
                    width: parent.width; height: 34 * view.unit
                    text: modelData.name
                    Accessible.name: text
                    contentItem: Text {
                        text: parent.text; color: "#f5f7fa"
                        font.family: "Interlude Variable"; font.pixelSize: Math.round(14 * view.unit)
                        verticalAlignment: Text.AlignVCenter; leftPadding: 10 * view.unit
                    }
                    background: Rectangle {
                        radius: 7 * view.unit
                        color: parent.hovered || parent.activeFocus ? "#467eabce" : "transparent"
                    }
                    onClicked: { powerPopup.close(); view.powerRequested(modelData.action); }
                }
            }
            Rectangle {
                visible: view.showSessionOptions; width: parent.width - 16 * view.unit
                x: 8 * view.unit; height: 1; color: "#30ffffff"
            }
            Repeater {
                model: view.showSessionOptions ? ["Sway", "Hyprland"] : []
                Button {
                    required property string modelData
                    width: parent.width; height: 30 * view.unit
                    text: (view.selectedSession === modelData ? "✓  " : "    ") + modelData
                    Accessible.name: modelData + " desktop session"
                    contentItem: Text {
                        text: parent.text; color: "#dce5ed"
                        font.family: "Interlude Variable"; font.pixelSize: Math.round(13 * view.unit)
                        verticalAlignment: Text.AlignVCenter; leftPadding: 10 * view.unit
                    }
                    background: Rectangle {
                        radius: 7 * view.unit
                        color: parent.hovered || parent.activeFocus ? "#467eabce" : "transparent"
                    }
                    onClicked: { view.sessionSelected(modelData); powerPopup.close(); }
                }
            }
        }
        onClosed: password.forceActiveFocus()
    }
}

import QtQuick
import Quickshell
import Quickshell.Io
import Quickshell.Wayland
import Quickshell.Services.Greetd

ShellRoot {
    id: root
    property string errorMessage: ""
    property string pending: ""
    property string prompt: "Enter Password"
    property bool busy: false
    property bool responseReady: false
    property bool echoResponse: false
    Component.onCompleted: Quickshell.watchFiles = false
    function submit(response) {
        if (busy) return;
        errorMessage = "";
        busy = true;
        if (responseReady) {
            responseReady = false;
            Greetd.respond(response);
        } else {
            pending = response;
            Greetd.createSession(Quickshell.env("LUMEN_AUTH_USER") || "user");
        }
    }
    Connections {
        target: Greetd
        function onAuthMessage(message, error, responseRequired, echoResponse) {
            if (error) root.errorMessage = message;
            if (!responseRequired) return;
            root.echoResponse = echoResponse;
            if (root.pending.length) {
                const response = root.pending;
                root.pending = "";
                Greetd.respond(response);
            } else {
                root.prompt = echoResponse ? message : "Enter Password";
                root.responseReady = true;
                root.busy = false;
            }
        }
        function onAuthFailure(message) {
            root.pending = ""; root.busy = false; root.responseReady = false;
            root.prompt = "Enter Password";
            root.errorMessage = "Password not recognised. Try again.";
        }
        function onError(error) {
            root.pending = ""; root.busy = false; root.responseReady = false;
            root.errorMessage = "Unable to sign in. Please try again.";
        }
        function onReadyToLaunch() {
            const command = [Quickshell.env("LUMEN_AUTH_SESSION") || "sway"];
            if (Quickshell.env("LUMEN_AUTH_UNSUPPORTED_GPU") === "1") command.push("--unsupported-gpu");
            Greetd.launch(command);
        }
    }
    Process {
        id: power
        onExited: (code, status) => { if (code !== 0) root.errorMessage = "Unable to change the power state."; }
    }
    Variants {
        model: Quickshell.screens
        PanelWindow {
            required property var modelData
            screen: modelData
            anchors { top: true; bottom: true; left: true; right: true }
            color: "#080b0f"
            WlrLayershell.keyboardFocus: WlrKeyboardFocus.Exclusive
            AuthView {
                anchors.fill: parent
                errorMessage: root.errorMessage
                prompt: root.prompt
                busy: root.busy
                echoResponse: root.echoResponse
                onSubmitted: response => root.submit(response)
                onPowerRequested: action => {
                    if (!["suspend", "reboot", "poweroff"].includes(action)) return;
                    power.command = ["/run/current-system/sw/bin/systemctl", action];
                    power.running = true;
                }
            }
        }
    }
}

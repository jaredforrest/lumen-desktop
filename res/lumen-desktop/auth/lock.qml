import QtQuick
import Quickshell
import Quickshell.Io
import Quickshell.Wayland
import Quickshell.Services.Pam

ShellRoot {
    id: root
    property string errorMessage: ""
    property string prompt: "Enter Password"
    property bool busy: false
    property string pending: ""
    Component.onCompleted: Quickshell.watchFiles = false
    function submit(response) {
        if (busy) return;
        errorMessage = "";
        busy = true;
        if (pam.active && pam.responseRequired) {
            pam.respond(response);
        } else {
            pending = response;
            if (!pam.start()) {
                pending = ""; busy = false;
                errorMessage = "Unable to authenticate. Please try again.";
            }
        }
    }
    PamContext {
        id: pam
        config: "swaylock"
        onPamMessage: {
            if (!responseRequired) return;
            if (root.pending.length) {
                const response = root.pending;
                root.pending = "";
                respond(response);
            } else {
                root.prompt = responseVisible ? message : "Enter Password";
                root.busy = false;
            }
        }
        onCompleted: result => {
            root.pending = "";
            root.busy = false;
            root.prompt = "Enter Password";
            if (result === PamResult.Success) {
                sessionLock.locked = false;
                quitTimer.start();
            } else {
                root.errorMessage = "Password not recognised. Try again.";
            }
        }
    }
    Timer { id: quitTimer; interval: 100; onTriggered: Qt.quit() }
    Process {
        id: power
        onExited: (code, status) => { if (code !== 0) root.errorMessage = "Unable to change the power state."; }
    }
    WlSessionLock {
        id: sessionLock
        locked: true
        WlSessionLockSurface {
            color: "#080b0f"
            AuthView {
                anchors.fill: parent
                errorMessage: root.errorMessage
                prompt: root.prompt
                busy: root.busy
                echoResponse: pam.active && pam.responseVisible
                onSubmitted: response => root.submit(response)
                onPowerRequested: action => {
                    if (!["suspend", "reboot", "poweroff"].includes(action)) return;
                    power.command = ["/run/current-system/sw/bin/systemctl", action];
                    power.running = true;
                }
            }
        }
    }
    // Expose readiness only; unlocking always requires PAM authentication.
    IpcHandler { target: "auth"; function secure(): bool { return sessionLock.secure; } }
}

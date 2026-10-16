pragma Singleton
import QtQuick
import Quickshell
QtObject {
    function commandFor(entry, newWindow) {
        let command = entry.command;
        if (newWindow) {
            // Use the application's declared action. Re-running its normal
            // command can only activate the existing instance (notably Codex).
            const actions = entry.actions || [];
            // Quickshell 0.2.1 desktopentry.cpp slices "Desktop Action " at
            // 16 instead of 15, dropping the first letter. Prefer correct IDs
            // so this also works after the upstream parser is fixed.
            const ids = ["new-window", "NewWindow", "NewDocument",
                         "ew-window", "ewWindow", "ewDocument"];
            for (let i = 0; i < ids.length; i++) {
                const action = actions.find(a => a.id === ids[i]);
                if (action && action.command.length) {
                    command = action.command;
                    break;
                }
            }
        }
        const argv = Array.from(command);
        return entry.runInTerminal ? ["foot", "-e"].concat(argv) : argv;
    }
    function runEntry(entry, newWindow) {
        run(commandFor(entry, newWindow), entry.workingDirectory);
    }
    function run(command, workingDirectory) {
        // Detaching a PID does not detach its systemd cgroup. App children must
        // survive a desktop-shell update, including Chromium zygote/GPU helpers.
        const argv=Quickshell.env("LUMEN_LAUNCH_SCOPES")==="1"
            ? ["lumen-launch-app","--"].concat(command) : command;
        Quickshell.execDetached({command:argv,workingDirectory:workingDirectory || ""});
    }
}

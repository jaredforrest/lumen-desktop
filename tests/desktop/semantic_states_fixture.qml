import QtQuick
import Quickshell
import "LUMEN_ASSETS" as D

ShellRoot {
    D.SettingsToggle { id: toggle; text: "Fixture permission"; checked: false }
    D.ControlTile { id: tile; label: "Fixture radio"; toggle: true; unavailable: false; active: false; subtitle: active ? "Connected" : "Disconnected" }
    D.StatusMessage { id: status; text: "Fixture request failed"; severity: "error" }
    D.DialogButton { id: remove; text: "Delete Fixture"; destructive: true; enabled: false }
    Timer {
        interval: 100
        running: true
        onTriggered: {
            try {
                if (toggle.checked || !toggle.checkable || toggle.Accessible.role !== Accessible.CheckBox) throw Error("Toggle semantics missing");
                toggle.checked = true;
                if (!toggle.checked) throw Error("Toggle state absent");
                tile.active = true;
                if (!tile.Accessible.checkable || !tile.Accessible.checked || !tile.Accessible.name.includes("Connected")) throw Error("Radio state conveyed only by colour");
                tile.unavailable = true;
                if (tile.activeFocusOnTab || !tile.Accessible.name.includes("unavailable")) throw Error("Unavailable state lacks semantic explanation");
                if (!status.Accessible.name.startsWith("Error: ") || status.Accessible.description !== "error") throw Error("Error is only a colour");
                status.severity = "success";
                if (!status.Accessible.name.startsWith("Success: ")) throw Error("Success is only a colour");
                status.severity = "warning";
                if (!status.Accessible.name.startsWith("Warning: ") || status.Accessible.description !== "warning") throw Error("Warning is only a colour");
                if (remove.enabled || !remove.destructive || remove.Accessible.name !== "Delete Fixture") throw Error("Destructive/disabled action identity missing");
                remove.enabled = true;
                remove.primary = true;
                if (remove.actionColor !== D.Theme.errorText || remove.contentItem.color !== D.Theme.destructiveText) throw Error("Filled destructive action bypasses semantic colours");
                remove.primary = false;
                if (remove.contentItem.color !== D.Theme.errorText) throw Error("Outlined destructive action bypasses semantic colours");
                console.log("PASS semantic control states");
            } catch (error) {
                console.error("FAIL " + error);
            }
            Qt.quit();
        }
    }
}

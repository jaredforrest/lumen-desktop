import QtQuick
import Quickshell
import "LUMEN_ASSETS" as D

ShellRoot {
    D.WidgetState { id: state }
    Timer {
        interval: 200
        running: true
        onTriggered: {
            try {
                const clamped = state.validate({desktopVisible: true, desktopColor: "bad", positions: {screen: {x: -2, y: 5}}});
                if (clamped.positions.screen.x !== 0 || clamped.positions.screen.y !== 1 || clamped.desktopColor !== "auto") throw Error("Invalid geometry accepted");
                state.setPreference("desktopVisible", true);
                state.setPreference("desktopStyle", "agenda");
                state.setPreference("desktopColor", "monochrome");
                state.setPosition("test-output", 0.25, 0.5);
                if (!state.preferences.calendarVisible || !state.preferences.desktopVisible || state.preferences.desktopStyle !== "agenda") throw Error("Widget preferences lost");
                console.log("FOCUS_EDITOR_PASS");
            } catch (error) { console.error("FOCUS_EDITOR_FAIL " + error); }
        }
    }
    Timer { interval: 600; running: true; onTriggered: Qt.quit() }
}

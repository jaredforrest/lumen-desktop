import QtQuick
import Quickshell
import Quickshell.Io

ShellRoot {
    FileView {
        id: file
        path: "FIXTURE_DATA_PATH"
        JsonAdapter {
            id: adapter
            property list<int> numbers: []
            property list<string> words: []
            property JsonObject details: JsonObject { property string label: "" }
            property var inlineJson: ({})
        }
        onSaved: { console.log("PASS typed JSON sequences serialized"); Qt.quit(); }
        onLoadFailed: { console.error("FAIL read fixture"); Qt.quit(); }
        onSaveFailed: { console.error("FAIL write fixture"); Qt.quit(); }
    }
    Timer {
        interval: 30
        running: true
        repeat: true
        onTriggered: {
            if (!file.loaded) return;
            stop();
            try {
                if (JSON.stringify(Array.from(adapter.numbers)) !== "[1,2,3]") throw Error("Numeric sequence lost");
                if (JSON.stringify(Array.from(adapter.words)) !== JSON.stringify(["one","two"])) throw Error("String sequence lost");
                if (adapter.details.label !== "loaded" || adapter.inlineJson.deep.ok !== true) throw Error("Nested object lost");
                adapter.numbers = [8, 5];
                adapter.words = ["literal # ü", "second"];
                adapter.details.label = "saved";
                adapter.inlineJson = {deep: {ok: false}};
                file.writeAdapter();
            } catch (error) {
                console.error("FAIL " + error);
                Qt.quit();
            }
        }
    }
}

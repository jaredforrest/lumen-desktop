import QtQuick
import QtQuick.Controls
import QtQuick.Window
import Quickshell
import "desktop" as D
ShellRoot {
    id: audit
    property var cases: @CASES@
    property var palettes: @PALETTES@
    property int index: 0
    property int stage: 0
    property bool painting: false
    property var records: []
    property var owner: null
    property var surface: null
    QtObject {
        id: stub
        property string popupOwner: ""
        signal dismissPopups(string owner)
        function preferredOutput() { return "" }
        function claimPopup(name) { popupOwner = name }
    }
    D.ForceQuitDialog { id: force; shell: stub; applicationName: "Private document application with a long name";
        phase: "error"; error: "The captured application changed while preparing the request. Close this decision and select the intended application again. No application has been terminated." }
    D.WindowPairPicker { id: pair; shell: stub;
        error: "The selected partner is no longer available. Choose another window to continue." }
    Window {
        id: displays
        visible: false
        color: D.Theme.colors.background
        D.FocusFlickable { id: displayScroll; anchors.fill: parent; anchors.margins: 16;
            contentHeight: pane.height; clip: true; flickableDirection: Flickable.VerticalFlick
            D.DisplaySettingsPane { id: pane; width: displayScroll.width }
            ScrollBar.vertical: D.DesktopScrollBar {}
        }
    }
    function check(ok, message) { if (!ok) throw Error(message + " " + JSON.stringify(cases[index])); }
    function walk(item, seen) {
        seen = seen || [];
        if (!item || seen.includes(item)) return [];
        seen.push(item);
        let result = [item], children = Array.from(item.children || []).concat(Array.from(item.data || []));
        if (item.contentItem) children.push(item.contentItem);
        for (const child of children) result = result.concat(walk(child, seen));
        return result;
    }
    function shown(item) {
        if (!(item instanceof Item)) return false;
        while (item && item !== surface) { if (!item.visible) return false; item = item.parent; }
        return item === surface;
    }
    function rect(item) { const p = item.mapToItem(surface, 0, 0); return {x:p.x,y:p.y,width:item.width,height:item.height}; }
    Timer { interval: 30; repeat: true; running: true
        onTriggered: { try {
            if (painting) return;
            if (index === cases.length) { console.log("ESSENTIAL_DIALOG_RESULT " + JSON.stringify(records)); Qt.quit(); return; }
            const c = cases[index];
            if (stage === 0) {
                force.visible = c.consumer === "Force";
                pair.visible = c.consumer === "Pair";
                displays.visible = c.consumer === "Displays";
                D.Theme.colors = palettes[c.mode];
                D.Theme.preferences = {textScale:c.scale, reduceMotion:true};
                owner = c.consumer === "Force" ? force : c.consumer === "Pair" ? pair : displays;
                surface = owner.contentItem;
                owner.width = c.width; owner.height = c.height;
                stage = 1; return;
            }
            if (stage === 1) { stage = 2; return; }
            const labels = walk(surface).filter(item => shown(item) && item.text && item.font && item.fontInfo);
            const expected = c.consumer === "Force" ? [["Force Quit ",17],["The captured application changed",13]] :
                c.consumer === "Pair" ? [["Choose a compatible partner",12],["The selected partner",12]] :
                [["Select a display, then drag",11],["Resolution and refresh rate",12],["Confirmed settings update",11]];
            const receipt = [];
            for (const [prefix, base] of expected) {
                const label = labels.find(item => item.text.startsWith(prefix));
                check(label, "Missing production label " + prefix);
                check(label.font.pixelSize === Math.round(base*c.scale), "Wrong role scale " + prefix);
                check(label.height+1 >= label.implicitHeight && label.width > 0, "Clipped label " + prefix);
                receipt.push({text:label.text,size:label.font.pixelSize,font:label.fontInfo.family,rect:rect(label)});
            }
            const actions = walk(surface).filter(item => shown(item) && item instanceof AbstractButton && item.enabled && item.text);
            check(actions.length, "No production action");
            const action = actions[actions.length-1];
            if (stage === 2) { action.forceActiveFocus(); stage = 3; return; }
            check(action.activeFocus, "Final action has no focus");
            const focus = rect(action);
            check(focus.y >= -1 && focus.y+focus.height <= surface.height+1, "Focused action outside surface");
            for (let parent = action.parent; parent && parent !== surface; parent = parent.parent)
                if (parent.clip) { const clip = rect(parent); check(focus.y >= clip.y-1 && focus.y+focus.height <= clip.y+clip.height+1, "Focused action clipped"); }
            const painted = walk(action).find(item => item.text && item.font && item.color);
            check(painted, "No painted final action label");
            painting = true;
            const capture = @OUTPUT@ + "/" + index + ".png";
            check(surface.grabToImage(result => { try {
                check(result.saveToFile(capture), "Capture save failed");
                records = records.concat([{case:c, capture:capture, focus:focus, painted:{rect:rect(painted),color:painted.color.toString()}, labels:receipt}]);
                index++; stage=0; painting=false;
            } catch (error) { console.error("ESSENTIAL_DIALOG_FAILURE " + error); Qt.quit(); } }, Qt.size(surface.width,surface.height)), "Capture rejected");
        } catch (error) { console.error("ESSENTIAL_DIALOG_FAILURE " + error); Qt.quit(); } }
    }
}

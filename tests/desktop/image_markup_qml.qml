import QtQuick
import Quickshell
import "assets" as D

ShellRoot {
    property int phase: 0
    property int ticks: 0
    property var staleIntent: null
    property var original: ({capability:"first",session:"epoch"})
    property var imageResult: ({type:"image",mime:"image/png",uri:"file://__IMAGE_PATH__",path:"__IMAGE_PATH__"})
    QtObject {
        id: service
        property var actionSelection: original
        property bool actionBusy: false
        property string actionOperation: ""
        property var resourceRetirement: null
        property var requests: []
        signal actionReceived(var value)
        function invoke(captured, operation, parameters) {
            if (operation === "image-markup") requests = requests.concat([{captured, operation, parameters}]);
            actionBusy = true; actionOperation = operation; return true;
        }
    }
    FloatingWindow {
        id: window
        visible: true
        implicitWidth: 680
        implicitHeight: 520
        D.ImageMarkupDialog { id: dialog; service: service; ownerWidth: window.width; ownerHeight: window.height }
    }
    function metadata() {
        service.actionBusy = false;
        service.actionReceived({operation:"image-markup-info",capability:"first",supported:true,width:80,height:60});
    }
    function check(value, message) { if (!value) throw Error(message); }
    Timer {
        interval: 30; running: true; repeat: true
        onTriggered: {
            try {
                if (++ticks > 100) throw Error("Markup protocol timeout");
                if (phase === 0) { check(dialog.begin(imageResult, original), "Image rejected"); metadata(); phase = 1; return; }
                if (phase === 1) {
                    if (!dialog.opened || !dialog.imageReady) return;
                    dialog.addRectangle(); staleIntent = dialog.captureSave();
                    check(dialog.strokes.length === 1, "No keyboard-equivalent rectangle");
                    dialog.cancel(); check(service.requests.length === 0, "Cancel wrote a file"); phase = 2; return;
                }
                if (phase === 2) {
                    if (dialog.visible) return;
                    check(dialog.sourceUri === "" && dialog.captured === null, "Closed image lease retained");
                    dialog.begin(imageResult, original); metadata(); phase = 3; return;
                }
                if (phase === 3) {
                    if (!dialog.opened || !dialog.imageReady) return;
                    dialog.addRectangle();
                    check(!dialog.saveTo("file:///synthetic/stale.png", staleIntent), "Old editor Save intent retargeted reopened editor");
                    const intent = dialog.captureSave();
                    dialog.saveTo("file:///synthetic/new.png", intent);
                    dialog.saveTo("file:///synthetic/duplicate.png", intent);
                    check(service.requests.length === 1, "Duplicate save dispatch");
                    const request = service.requests[0];
                    check(request.operation === "image-markup" && request.captured.capability === "first" && request.parameters.strokes[0].tool === "rectangle", "Save retargeted");
                    service.actionBusy = false;
                    service.actionReceived({operation:"image-markup",capability:"other",saved:true});
                    check(dialog.visible, "Unrelated acknowledgement closed editor");
                    service.actionReceived({operation:"image-markup",capability:"first",error:"Destination exists"});
                    check(dialog.visible && dialog.message === "Destination exists" && dialog.strokes.length === 1, "Failed Save lost edits");
                    service.actionSelection = {capability:"replacement",session:"epoch"}; phase = 4; return;
                }
                if (phase === 4) {
                    if (dialog.visible) return;
                    check(dialog.sourceUri === "", "Selection replacement retained old image");
                    check(service.requests.length === 1, "Selection change dispatched");
                    check(!dialog.begin({type:"image",mime:"application/pdf"}, original), "PDF editor falsely advertised");
                    console.log("IMAGE_MARKUP_QT_PASS"); Qt.quit();
                }
            } catch (error) { console.error("IMAGE_MARKUP_QT_FAIL " + error); Qt.quit(); }
        }
    }
}

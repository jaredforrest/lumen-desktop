import QtQuick
import QtQuick.Controls
import QtQuick.Window
import Quickshell
import Quickshell.Io
Row {
    id: root
    visible: !Theme.nativeWindowFrame
    property bool windowActive: true
    property string windowName: "Window"
    property bool canMinimize: true
    property bool canFullscreen: true
    // All desktop-owned windows use the same inset and traffic-light geometry.
    x: 10
    y: 14
    readonly property bool controlsHovered: [0,1,2].some(i=>{const b=lights.itemAt(i);return b && (b.hovered || b.pointerHovered);})
    signal triggered(int action)
    property var capturedWindow:null
    property bool greenHovered:false
    property int captureEpoch:0
    property bool recaptureRequested:false
    function capture() {
        if(!Theme.nativeWindowActions || !root.Window.window)return;
        if(captureProcess.running){if(captureProcess.epoch!==captureEpoch)recaptureRequested=true;return;}
        captureEpoch++;captureProcess.epoch=captureEpoch;
        captureProcess.command=["lumen-focus-window","--capture","org.quickshell",root.Window.window.title];
        captureProcess.running=true;
    }
    function hoverGreen(value) {
        greenHovered=value;
        if(value){if(capturedWindow)sendHover();else capture();}
        else {
            captureEpoch++;
            if(capturedWindow)Quickshell.execDetached(["lumen-focus-window","--captured",JSON.stringify(capturedWindow),"--layout-leave"]);
        }
    }
    function sendHover() {
        if(!greenHovered || !capturedWindow || !root.Window.window || !root.canFullscreen)return;
        const green=lights.itemAt(2),point=green.mapToItem(root.Window.window.contentItem,green.width/2,green.height/2);
        Quickshell.execDetached(["lumen-focus-window","--captured",JSON.stringify(capturedWindow),"--layout-hover",String(point.x),String(point.y)]);
    }
    function activate(index, modifiers, captured) {
        if (Theme.nativeWindowActions && root.Window.window) {
            if(!captured){capture();return;}
            const action = index === 2 && (modifiers & Qt.AltModifier) ? "fill" : ["close","minimize","fullscreen"][index];
            Quickshell.execDetached(["lumen-focus-window","--captured",JSON.stringify(captured),"--action",action]);
        } else root.triggered(index);
    }
    Component.onCompleted:Qt.callLater(capture)
    Component.onDestruction:if(capturedWindow)Quickshell.execDetached(["lumen-focus-window","--captured",JSON.stringify(capturedWindow),"--layout-leave"])
    onVisibleChanged:if(visible)Qt.callLater(capture);else hoverGreen(false)
    onCanFullscreenChanged:if(!canFullscreen)hoverGreen(false)
    Process {
        id:captureProcess
        property int epoch:0
        onExited:if(root.recaptureRequested){root.recaptureRequested=false;Qt.callLater(root.capture);}
        stdout:StdioCollector {onStreamFinished:{
            if(captureProcess.epoch!==root.captureEpoch)return;
            try {
                const value=JSON.parse(text);
                if(value.version!==1 || !Number.isSafeInteger(value.conId) || value.conId<=0 || !Number.isSafeInteger(value.pid) || value.pid<=0 || typeof value.foreignId!=="string" || !/^[0-9a-f]{32}$/.test(value.foreignId) || value.appId!=="org.quickshell" || value.key!==value.conId+":"+value.pid+":"+value.foreignId)return;
                root.capturedWindow=value;
                if(root.greenHovered)root.sendHover();
            }catch(error){}
        }}
    }
    spacing: 0
    Repeater {
        id:lights
        model: ["#ff5f57", "#febc2e", "#28c840"]
        AbstractButton {
            id: button
            required property int index
            required property string modelData
            property var pressedCapture:null
            readonly property bool pointerHovered:input.containsMouse
            width: 20; height: 20
            enabled: index === 0 || (index === 1 ? root.canMinimize : root.canFullscreen)
            Accessible.name: ["Close " + root.windowName, "Minimize " + root.windowName, "Enter full screen"][index]
            background: Rectangle {
                anchors.centerIn: parent
                width: 12; height: 12
                radius: 6
                color: button.enabled && root.windowActive ? button.modelData : Theme.dark ? "#555559" : "#ceced2"
                border.width: 1
                border.color: button.activeFocus ? Theme.colors.accent : Qt.darker(color, 1.12)
                FocusOutline {focused:button.activeFocus;inset:-3;radius:9}
            }
            contentItem: Item {
                SystemGlyph {
                    visible: button.enabled && (root.controlsHovered || button.activeFocus)
                    anchors.centerIn: parent
                    name: button.index === 0 ? "close" : button.index === 1 ? "minus" : "plus"
                    width: 8; height: 8; ink: "#333333"
                }
            }
            onActiveFocusChanged:if(activeFocus)root.capture()
            onPressed:pressedCapture=root.capturedWindow
            onClicked:root.activate(index,0,pressedCapture)
            MouseArea {
                id:input
                anchors.fill:parent
                enabled:button.enabled
                hoverEnabled:true
                onEntered:{root.capture();if(button.index===2)root.hoverGreen(true);}
                onExited:if(button.index===2)root.hoverGreen(false)
                onPressed:mouse=>{button.pressedCapture=root.capturedWindow;if(Theme.nativeWindowActions && !button.pressedCapture)root.capture();}
                onClicked:mouse=>root.activate(button.index,mouse.modifiers,button.pressedCapture)
            }
        }
    }
}

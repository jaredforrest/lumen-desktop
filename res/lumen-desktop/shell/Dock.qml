import QtQuick
import QtQuick.Effects
import Quickshell
import Quickshell.Wayland

PanelWindow {
    id: dock
    property string outputName:""
    Component.onCompleted:outputName=screen.name
    required property var shell
    property int hovered: -1
    property bool keyboardMode:false
    property int keyboardIndex:0
    function leaveKeyboard(){keyboardMode=false;hovered=-1;if(shell.popupOwner==="dock:"+outputName)shell.claimPopup("");}
    Connections {target:shell;function onRequestDockFocus(name){if(name===dock.outputName){shell.claimPopup("dock:"+dock.outputName);dock.keyboardMode=true;dock.hovered=dock.keyboardIndex;keyboard.focus=true;}}}
    property real pointerSlot: -1
    function trackPointer(x, y) {
        keyboardMode = false;
        const position = (x - 12) / slot;
        if (y < 0 || y >= 100 || position < 0 || position >= shell.referenceDock.length) {
            hovered = -1; pointerSlot = -1;
            return;
        }
        hovered = Math.floor(position);
        pointerSlot = position - 0.5;
    }
    function clearPointer() { if (!keyboardMode) { hovered = -1; pointerSlot = -1; } }
    readonly property real artworkScale: iconSize/68
    property var contextEntry: null
    property real contextX: 0
    property int contextIndex:-1
    function stepContext(delta) {
        const count=contextItems.count;if(!count)return;
        contextIndex=contextIndex<0?(delta>0?0:count-1):(contextIndex+delta+count)%count;
        contextScroll.contentY=Math.max(0,Math.min(contextScroll.contentHeight-contextScroll.height,contextIndex*(Theme.menuRowHeight+2)-contextScroll.height/2));
    }
    function activateContext(row) {
        const entry=contextEntry;contextEntry=null;
        if(row.win)shell.focusWindow(row.win);else if(row.action==="new")shell.activate(entry,true);else shell.windowsFor(entry).forEach(w=>w.close());
    }
    onContextEntryChanged: {
        contextIndex=-1;
        if(contextEntry)shell.claimPopup("dock:"+outputName);
        else if(shell.popupOwner==="dock:"+outputName)shell.popupOwner="";
    }
    Connections {target:shell; function onDismissPopups(owner) {if(owner!=="dock:"+dock.outputName){dock.contextEntry=null;dock.keyboardMode=false;dock.hovered=-1;}} }
    readonly property real slot: (width-24)/Math.max(1,shell.referenceDock.length)
    readonly property real iconSize: Math.min(48,slot-8)
    anchors { bottom: true }
    margins.bottom: 7
    implicitWidth: Math.min((screen?screen.width:1440)-48, shell.referenceDock.length*58+24)
    implicitHeight: 128
    exclusiveZone: 81
    color: "transparent"
    WlrLayershell.namespace: "lumen-dock"
    WlrLayershell.keyboardFocus:keyboardMode?WlrKeyboardFocus.Exclusive:WlrKeyboardFocus.None
    Item {id:keyboard;focus:dock.keyboardMode
        Keys.onPressed:event=>{
            if(!dock.keyboardMode)return;
            if(event.key===Qt.Key_Escape){dock.leaveKeyboard();event.accepted=true;}
            else if([Qt.Key_Left,Qt.Key_Right,Qt.Key_Home,Qt.Key_End].includes(event.key)){
                const n=shell.referenceDock.length;
                dock.keyboardIndex=event.key===Qt.Key_Home?0:event.key===Qt.Key_End?n-1:(dock.keyboardIndex+(event.key===Qt.Key_Right?1:-1)+n)%n;
                dock.hovered=dock.keyboardIndex;event.accepted=true;
            } else if(event.key===Qt.Key_Return || event.key===Qt.Key_Space){const item=shell.referenceDock[dock.keyboardIndex];dock.leaveKeyboard();shell.activateDock(item,!!(event.modifiers & Qt.ShiftModifier),dock.outputName);event.accepted=true;}
            else if(event.key===Qt.Key_Up || event.key===Qt.Key_Menu){const item=shell.referenceDock[dock.keyboardIndex];if(item.entry){dock.keyboardMode=false;dock.contextEntry=item.entry;dock.contextX=((dock.screen?dock.screen.width:1440)-dock.width)/2+12+(dock.keyboardIndex+0.5)*dock.slot;}event.accepted=true;}
        }
    }
    // Keep the shelf/icon hit region stable while moving between neighbours.
    mask: Region { x:0; y:28; width:dock.width; height:100 }
    // In Sway only, sample our installed wallpaper to soften its detail under
    // the shelf. This is wallpaper-only blur, not blur of other applications.
    Item {
        anchors.bottom: parent.bottom; width:parent.width; height:74
        visible: !Theme.preferences.reduceTransparency && !Theme.compositorEffects
        Image { id:backdrop; width:(dock.screen?dock.screen.width:1440); height:(dock.screen?dock.screen.height:1000); source:Qt.resolvedUrl("wallpaper.png"); fillMode:Image.PreserveAspectCrop; visible:false }
        ShaderEffectSource { id:slice; sourceItem:backdrop; sourceRect:Qt.rect(((dock.screen?dock.screen.width:1440)-dock.width)/2,(dock.screen?dock.screen.height:1000)-81,dock.width,74); width:dock.width; height:74; visible:false }
        Rectangle { id:roundedMask; width:dock.width; height:74; radius:22; layer.enabled:true; visible:false }
        MultiEffect { anchors.fill:parent; source:slice; blurEnabled:true; blurMax:32; blur:0.7; maskEnabled:true; maskSource:roundedMask; autoPaddingEnabled:false }
    }
    Rectangle {
        anchors.bottom:parent.bottom; width:parent.width; height:74; radius:22
        color:Theme.preferences.reduceTransparency?Theme.colors.background:Theme.compositorEffects ? (Theme.dark?"#b8323236":"#c8dedee3") : (Theme.dark?"#e6323236":"#e6dedee3")
        border.color:Theme.outline; border.width:1
    }
    Item {
        width: parent.width; anchors.bottom:parent.bottom; height:74
        Repeater {
            model:shell.referenceDock
            Item {
                id: tile
                required property var modelData
                required property int index
                x:12+index*dock.slot; width:dock.slot; height:74
                readonly property var appWindows: shell.dockWindows(modelData)
                Rectangle {visible:tile.modelData.key==="downloads";x:-5;y:15;width:1;height:42;color:Theme.outline}
                readonly property real magnification: !Theme.preferences.dockMagnification || dock.hovered<0 ? 0 : Math.max(0,1-Math.abs(index-dock.pointerSlot)*0.48)
                Rectangle {anchors.fill:parent;anchors.margins:2;radius:10;color:"transparent";border.width:2;border.color:Theme.colors.accent;visible:dock.keyboardMode && dock.keyboardIndex===tile.index}
                AppIcon {
                    id: icon
                    anchors.horizontalCenter:parent.horizontalCenter; anchors.bottom:parent.bottom; anchors.bottomMargin:13
                    width:tile.modelData.canvas*dock.artworkScale+18*tile.magnification; height:width
                    iconSource:tile.modelData.key==="downloads" ? Qt.resolvedUrl("icons/folder.svg") : tile.modelData.image ? Qt.resolvedUrl("reference-icons/"+tile.modelData.image) : Quickshell.iconPath(tile.modelData.entry.icon,true)
                    calendarDate:shell.currentDate

                    Behavior on width { NumberAnimation { duration:Theme.preferences.reduceMotion?0:150; easing.type:Easing.OutCubic } }
                }
                Rectangle { anchors.horizontalCenter:parent.horizontalCenter; y:66; width:4; height:4; radius:2; color:Theme.colors.text; visible:tile.appWindows.length>0 }
                Rectangle {
                    visible:dock.hovered===tile.index; anchors.horizontalCenter:parent.horizontalCenter; y:-38
                    width:tip.implicitWidth+20; height:26; radius:7; color:Theme.colors.background; border.color:Theme.colors.border
                    Text { id:tip; anchors.centerIn:parent; text:shell.dockLabel(tile.modelData); color:Theme.colors.text; font.family:Theme.font; font.pixelSize:13 }
                }
                Accessible.role:Accessible.Button; Accessible.name:shell.dockLabel(modelData)
                Accessible.onPressAction:shell.activateDock(modelData,false,dock.outputName)
            }
        }
    }
    // A single stable hit area avoids pointer-grab and region transitions between tiles.
    MouseArea {
        id: pointerArea
        x: 0; y: 28; width: dock.width; height: 100
        hoverEnabled: true; acceptedButtons: Qt.LeftButton | Qt.RightButton
        onEntered: dock.trackPointer(mouseX, mouseY)
        onPositionChanged: mouse => dock.trackPointer(mouse.x, mouse.y)
        onExited: dock.clearPointer()
        onCanceled: dock.clearPointer()
        onReleased: if (!containsMouse) dock.clearPointer()
        onClicked: mouse => {
            dock.trackPointer(mouse.x, mouse.y);
            if (dock.hovered < 0) return;
            const index = dock.hovered;
            const item = shell.referenceDock[index];
            if (mouse.button === Qt.RightButton && item.entry) {
                dock.contextEntry = item.entry;
                dock.contextX = ((dock.screen ? dock.screen.width : 1440) - dock.width) / 2 + 12 + (index + 0.5) * dock.slot;
            } else shell.activateDock(item, !!(mouse.modifiers & Qt.ShiftModifier), dock.outputName);
        }
    }

    PanelWindow {
        screen:dock.screen; visible:dock.contextEntry!==null
        anchors {top:true;bottom:true;left:true;right:true}
        exclusionMode:ExclusionMode.Ignore; color:"transparent"
        WlrLayershell.layer:WlrLayer.Overlay; WlrLayershell.namespace:"lumen-dock-popup"
        WlrLayershell.keyboardFocus:WlrKeyboardFocus.Exclusive
        MouseArea {anchors.fill:parent;onClicked:dock.contextEntry=null}
        Item {anchors.fill:parent;focus:true;Keys.onEscapePressed:dock.contextEntry=null
            Keys.onDownPressed:dock.stepContext(1)
            Keys.onUpPressed:dock.stepContext(-1)
            Keys.onReturnPressed:if(dock.contextIndex>=0)dock.activateContext(contextItems.model[dock.contextIndex])
        }
        PanelSurface {
            id:contextPanel
            FontMetrics {id:contextMetrics;font.family:Theme.font;font.pixelSize:13}
            x:Math.max(8,Math.min(parent.width-width-8,dock.contextX-width/2)); y:Math.max(Theme.barHeight+8,parent.height-height-92)
            width:Math.max(220,Math.min(380,contextItems.model.reduce((w,n)=>Math.max(w,contextMetrics.advanceWidth(n.label)+36),0)));height:Math.min(contextColumn.height+16,parent.height-Theme.barHeight-108);radius:Theme.menuRadius;color:Theme.colors.background;border.color:Theme.outline
            MouseArea {anchors.fill:parent}
            Flickable {
                id:contextScroll;anchors.fill:parent;anchors.margins:8;contentWidth:width;contentHeight:contextColumn.height;clip:true;boundsBehavior:Flickable.StopAtBounds
            Column {
                id:contextColumn;spacing:2
                Repeater {
                    id:contextItems
                    model:dock.contextEntry ? [{label:"Open New",action:"new"}].concat(shell.windowsFor(dock.contextEntry).map(w=>({label:(w.minimized?"Minimized — ":"")+(w.title || "Window"),win:w}))).concat([{label:"Close Windows",action:"close"}]) : []
                    Rectangle {
                        required property var modelData
                        required property int index
                        width:contextPanel.width-16;height:Theme.menuRowHeight;radius:5;color:dock.contextIndex===index?Theme.colors.accent:"transparent"
                        Text {anchors.fill:parent;anchors.leftMargin:10;anchors.rightMargin:10;verticalAlignment:Text.AlignVCenter;text:modelData.label;elide:Text.ElideRight;color:dock.contextIndex===index?Theme.colors.selectedText:Theme.colors.text;font.family:Theme.font;font.pixelSize:13}
                        MouseArea {id:contextMouse;anchors.fill:parent;hoverEnabled:true;onEntered:dock.contextIndex=index;onClicked:dock.activateContext(modelData)}
                    }
                }
            }
        }
    }
}
}

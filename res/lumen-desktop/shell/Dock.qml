import QtQuick
import QtQuick.Controls
import QtQuick.Effects
import Quickshell
import Quickshell.Wayland
import Quickshell.Io
import "DockInteraction.js" as DockInteraction

PanelWindow {
    id: dock
    WallpaperSettingsState {id:wallpaperState;active:GraphicsInfo.api !== GraphicsInfo.Software && !Theme.compositorEffects && !Theme.preferences.reduceTransparency}
    readonly property string position:Theme.preferences.dockPosition || "bottom"
    readonly property bool vertical:position!=="bottom"
    readonly property bool autoHide:Theme.preferences.dockAutoHide===true
    property bool revealed:true
    readonly property real requestedSize:Theme.preferences.dockSize || 48
    readonly property real panelThickness:requestedSize+80
    readonly property real shelfThickness:iconSize+26
    property string outputName:""
    Component.onCompleted:{outputName=screen.name;if(autoHide)hideTimer.restart();}
    required property var shell
    readonly property bool displayAllowed:!shell.displaySpaces || shell.displaySpaces.dockVisible(outputName)
    visible:displayAllowed
    onDisplayAllowedChanged:if(!displayAllowed){cancelDrag();leaveKeyboard();contextEntry=null;contextAlias=null;hovered=-1;}
    property int hovered: -1
    property bool keyboardMode:false
    property int keyboardIndex:0
    function leaveKeyboard(){keyboardMode=false;hovered=-1;if(shell.popupOwner==="dock:"+outputName)shell.claimPopup("");}
    Connections {target:shell;function onRequestDockFocus(name){if(dock.displayAllowed && (shell.displaySpaces?shell.displaySpaces.dockOutput(name):name)===dock.outputName){shell.claimPopup("dock:"+dock.outputName);dock.keyboardMode=true;dock.hovered=dock.keyboardIndex;keyboard.focus=true;}}}
    property real pointerSlot: -1
    property string pressedKey: ""
    property real pressX:0
    property real pressY:0
    property var pressIntent:null
    function cancelDrag(){pressedKey="";pressIntent=null;dragging=false;dragCompleted=true;}
    function removableDrag(){const item=dragIntentValid()?pressIntent.item:null;return item && (["file","folder"].includes(item.action) || (item.action!=="minimized-window" && item.entry && shell.canConfigureDockEntry(item.entry) && shell.isDockPinned(item.entry)));}
    function dragIntentValid(){return pressIntent && pressIntent.items===shell.referenceDock && !shell.dockPreferencesBusy && shell.referenceDock.includes(pressIntent.item);}
    Connections {target:shell;function onDockPreferencesBusyChanged(){if(shell.dockPreferencesBusy && dock.pressIntent)dock.cancelDrag();}function onReferenceDockChanged(){if(dock.pressIntent && !dock.dragIntentValid())dock.cancelDrag();}}
    property bool dragging:false
    property bool dragCompleted:false
    property string dropError:""
    function refuseBusyDrop(){dropError="Wait for the current file drop to finish";dropErrorTimer.restart();}
    function dropFiles(entry,urls){
        if(dropWorker.running){refuseBusyDrop();return false;}
        const list=Array.from(urls).map(url=>url.toString()),payload=JSON.stringify(list);
        if(list.length>100 || payload.length>65536){dropError="Drop at most 100 files with shorter paths";dropErrorTimer.restart();return false;}
        dropError="";dropWorker.command=["lumen-dock-drop",entry.id,payload];dropWorker.running=true;return true;
    }
    Process {id:dropWorker;stdout:StdioCollector {onStreamFinished:{try{const result=JSON.parse(text);dock.dropError=result.error || "";}catch(e){dock.dropError="The application could not open this drop";}dropErrorTimer.restart();}}}
    Timer {id:dropErrorTimer;interval:6000;onTriggered:dock.dropError=""}
    function trackPointer(x, y) {
        keyboardMode = false;
        const position = (x - 12) / slot;
        if (y < 0 || y >= surface.height-28 || position < 0 || position >= shell.referenceDock.length) {
            hovered = -1; pointerSlot = -1;
            return;
        }
        hovered = dock.position==="right"?shell.referenceDock.length-1-Math.floor(position):Math.floor(position);
        pointerSlot = dock.position==="right"?shell.referenceDock.length-0.5-position:position-0.5;
    }
    function clearPointer() { if (!keyboardMode) { hovered = -1; pointerSlot = -1; } }
    readonly property real artworkScale: iconSize/68
    property var contextEntry: null
    property var contextAlias:null
    property real contextX: 0
    property real contextY:0
    property int contextIndex:-1
    property var contextSelection:null
    property var contextWindowSnapshot:[]
    property string contextInstanceSnapshot:""
    property int contextGeneration:0
    function contextValid(){if(!contextEntry)return true;if(typeof shell.applicationInstanceKey==="function" && shell.applicationInstanceKey(contextEntry)!==contextInstanceSnapshot)return false;const live=shell.windowsFor(contextEntry);return live.length===contextWindowSnapshot.length && contextWindowSnapshot.every(w=>w && live.includes(w));}
    Connections {target:shell;ignoreUnknownSignals:true;function onWindowsChanged(){if(dock.contextEntry && !dock.contextValid())dock.closeContext();}function onRunningApplicationsChanged(){if(dock.contextEntry && !dock.contextValid())dock.closeContext();}}
    function closeContext(){contextGeneration++;contextEntry=null;contextAlias=null;contextSelection=null;}
    function captureContext(row,generation){return {generation:generation===undefined?contextGeneration:generation,row:row,entry:contextEntry,appId:contextEntry?contextEntry.id:"",instanceKey:contextEntry && typeof shell.applicationInstanceKey==="function"?shell.applicationInstanceKey(contextEntry):"",alias:contextAlias,aliasKey:contextAlias?contextAlias.key:"",windows:contextEntry?shell.windowsFor(contextEntry).slice():[]};}
    function contextSelected(row){return !!contextSelection && (row.action==="window"?contextSelection.row.action==="window" && row.win===contextSelection.row.win:row.action===contextSelection.row.action);}
    function selectContext(index){contextIndex=index;const row=contextItems.model[index];contextSelection=row?captureContext(row):null;}
    function openContext(item,index){
        if(item.action==="minimized-window")return;
        contextGeneration++;contextIndex=-1;contextSelection=null;
        contextWindowSnapshot=item.entry?shell.windowsFor(item.entry).slice():[];contextInstanceSnapshot=item.entry && typeof shell.applicationInstanceKey==="function"?shell.applicationInstanceKey(item.entry):"";
        dock.keyboardMode=false;contextEntry=item.entry || null;contextAlias=item.entry?null:item;
        contextY=((screen?screen.height:1000)-height)/2+12+(index+0.5)*slot;
        contextX=((screen?screen.width:1440)-width)/2+12+(index+0.5)*slot;
    }
    onContextAliasChanged:{contextIndex=-1;contextSelection=null;if(contextAlias)shell.claimPopup("dock:"+outputName);else if(!contextEntry){if(shell.popupOwner==="dock:"+outputName)shell.popupOwner="";hideTimer.restart();}}
    function stepContext(delta) {
        if(!contextValid()){closeContext();return;}
        const count=contextItems.count;if(!count)return;
        let next=contextIndex;
        for(let i=0;i<count;i++){next=next<0?(delta>0?0:count-1):(next+delta+count)%count;if(contextItems.model[next].enabled!==false)break;}
        if(contextItems.model[next].enabled===false)return;
        selectContext(next);
        contextScroll.contentY=Math.max(0,Math.min(contextScroll.contentHeight-contextScroll.height,next*(Theme.menuRowHeight+2)-contextScroll.height/2));
        const item=contextItems.itemAt(next);if(item)item.forceActiveFocus();
    }
    function applicationRunning(entry){return typeof shell.runningFor==="function"?shell.runningFor(entry):shell.windowsFor(entry).length>0;}
    function activateContext(row,captured) {
        if(!row || row.enabled===false)return;
        const intent=captured || captureContext(row),entry=intent.entry,alias=intent.alias;
        if(intent.generation!==contextGeneration || entry!==contextEntry || alias!==contextAlias || (entry && entry.id!==intent.appId) || (alias && alias.key!==intent.aliasKey))return;
        if(entry && !contextValid()){closeContext();return;}
        const currentInstances=entry && typeof shell.applicationInstanceKey==="function"?shell.applicationInstanceKey(entry):"";
        if(row.action==="quit" && !intent.instanceKey)return;
        if(["quit","force-quit"].includes(row.action) && currentInstances!==intent.instanceKey)return;
        if(row.action==="force-quit" && (intent.windows.length!==shell.windowsFor(entry).length || intent.windows.some(w=>!shell.windowsFor(entry).includes(w))))return;
        closeContext();
        if(alias){if(row.action==="alias-open")shell.activateDock(alias,false,outputName);else if(row.action==="alias-reveal")shell.revealDockAlias(alias);else if(row.action==="alias-remove")shell.removeDockAlias(alias);return;}
        if(!entry)return;
        const live=shell.windowsFor(entry);
        if(row.action==="window"){if(row.win && live.includes(row.win))shell.focusWindow(row.win);}
        else if(row.action==="new")shell.activate(entry,true);
        else if(row.action==="force-quit")shell.forceQuit(entry);
        else if(row.action==="quit")shell.quitApplication(entry,intent.instanceKey);
        else if(["pin","remove","earlier","later"].includes(row.action))shell.updateDockPin(entry,row.action);
        else if(row.action==="close")intent.windows.filter(w=>w && live.includes(w)).forEach(w=>w.close());
    }
    onContextEntryChanged: {
        contextIndex=-1;contextSelection=null;contextWindowSnapshot=contextEntry?shell.windowsFor(contextEntry).slice():[];contextInstanceSnapshot=contextEntry && typeof shell.applicationInstanceKey==="function"?shell.applicationInstanceKey(contextEntry):"";
        if(contextEntry)shell.claimPopup("dock:"+outputName);
        else {if(shell.popupOwner==="dock:"+outputName)shell.popupOwner="";hideTimer.restart();}
    }
    Connections {target:shell; function onDismissPopups(owner) {if(owner!=="dock:"+dock.outputName){dock.contextEntry=null;dock.contextAlias=null;dock.keyboardMode=false;dock.hovered=-1;hideTimer.restart();}} }
    readonly property real slot: (surface.width-24)/Math.max(1,shell.referenceDock.length)
    readonly property real iconSize: Math.max(12,Math.min(requestedSize,slot-8))
    anchors { bottom:!dock.vertical;left:dock.position==="left";right:dock.position==="right" }
    margins.bottom: !vertical && !autoHide?7:0
    margins.left: position==="left" && !autoHide?7:0
    margins.right: position==="right" && !autoHide?7:0
    implicitWidth: vertical?panelThickness:Math.min((screen?screen.width:1440)-48,shell.referenceDock.length*(requestedSize+10)+24)
    implicitHeight: vertical?Math.min((screen?screen.height:1000)-64,shell.referenceDock.length*(requestedSize+10)+24):panelThickness
    exclusiveZone: autoHide?0:shelfThickness+7
    Timer {id:hideTimer;interval:500;onTriggered:if(dock.autoHide && !dock.keyboardMode && !dock.contextEntry && !dock.contextAlias && !dock.dragging)dock.revealed=false}
    onAutoHideChanged:{revealed=true;if(autoHide)hideTimer.restart();}
    onKeyboardModeChanged:if(keyboardMode){revealed=true;hideTimer.stop();}
    color: "transparent"
    WlrLayershell.namespace: "lumen-dock"
    WlrLayershell.keyboardFocus:(keyboardMode || dragging)?WlrKeyboardFocus.Exclusive:WlrKeyboardFocus.None
    Item {id:keyboard;focus:dock.keyboardMode || dock.dragging
        Keys.onPressed:event=>{
            if(dock.dragging){
                if(event.key===Qt.Key_Escape){dock.cancelDrag();}
                event.accepted=true;return;
            }
            if(!dock.keyboardMode)return;
            if(event.key===Qt.Key_Escape){dock.leaveKeyboard();event.accepted=true;}
            else if([Qt.Key_Home,Qt.Key_End].includes(event.key) || (dock.vertical?[Qt.Key_Up,Qt.Key_Down]:[Qt.Key_Left,Qt.Key_Right]).includes(event.key)){
                const n=shell.referenceDock.length;
                dock.keyboardIndex=event.key===Qt.Key_Home?0:event.key===Qt.Key_End?n-1:(dock.keyboardIndex+([Qt.Key_Right,Qt.Key_Down].includes(event.key)?1:-1)+n)%n;
                dock.hovered=dock.keyboardIndex;event.accepted=true;
            } else if(event.key===Qt.Key_Return || event.key===Qt.Key_Space){const item=shell.referenceDock[dock.keyboardIndex];dock.leaveKeyboard();shell.activateDock(item,!!(event.modifiers & Qt.ShiftModifier),dock.outputName,event.modifiers);event.accepted=true;}
            else if(event.key===Qt.Key_Menu || (!dock.vertical && event.key===Qt.Key_Up) || (dock.vertical && event.key===(dock.position==="left"?Qt.Key_Right:Qt.Key_Left))){const item=shell.referenceDock[dock.keyboardIndex];if(item.entry || ["file","folder"].includes(item.action))dock.openContext(item,dock.keyboardIndex);event.accepted=true;}
        }
    }
    // Keep the shelf/icon hit region stable while moving between neighbours.
    mask: Region {
        x:dock.vertical?(dock.position==="left"?0:(dock.autoHide && !dock.revealed?dock.width-6:0)):0
        y:dock.vertical?0:(dock.autoHide && !dock.revealed?dock.height-6:28)
        width:dock.vertical && dock.autoHide && !dock.revealed?6:dock.width
        height:!dock.vertical && dock.autoHide && !dock.revealed?6:dock.height-(dock.vertical?0:28)
    }
    MouseArea {anchors.fill:parent;z:100;visible:dock.autoHide && !dock.revealed;hoverEnabled:true
        onEntered:{dock.revealed=true;hideTimer.stop();}
        onPositionChanged:{dock.revealed=true;hideTimer.stop();}
    }
    Item {id:surface;width:dock.vertical?dock.height:dock.width;height:dock.vertical?dock.width:dock.height;anchors.centerIn:parent
        rotation:dock.position==="left"?90:dock.position==="right"?-90:0
        opacity:!dock.autoHide || dock.revealed?1:0
        Behavior on opacity {NumberAnimation {duration:Theme.preferences.reduceMotion?0:140}}

    // In Sway only, sample the selected wallpaper to soften its detail under
    // the shelf. This is wallpaper-only blur, not blur of other applications.
    Item {
        anchors.bottom: parent.bottom; width:parent.width; height:dock.shelfThickness
        visible: GraphicsInfo.api !== GraphicsInfo.Software && !dock.vertical && !Theme.preferences.reduceTransparency && !Theme.compositorEffects
        Image { id:backdrop; width:(dock.screen?dock.screen.width:1440); height:(dock.screen?dock.screen.height:1000); source:{const row=wallpaperState.outputs.find(item=>dock.screen && item.name===dock.screen.name);return row && row.image && !row.fallback?"file://"+encodeURI(row.image).replace(/#/g,"%23").replace(/\?/g,"%3F"):Qt.resolvedUrl("wallpaper.png");} fillMode:Image.PreserveAspectCrop; visible:false }
        ShaderEffectSource { id:slice; sourceItem:backdrop; sourceRect:Qt.rect(((dock.screen?dock.screen.width:1440)-dock.width)/2,(dock.screen?dock.screen.height:1000)-dock.shelfThickness-7,surface.width,dock.shelfThickness); width:surface.width; height:dock.shelfThickness; visible:false }
        Rectangle { id:roundedMask; width:surface.width; height:dock.shelfThickness; radius:22; layer.enabled:GraphicsInfo.api !== GraphicsInfo.Software; visible:false }
        MultiEffect { anchors.fill:parent; source:slice; blurEnabled:true; blurMax:32; blur:0.7; maskEnabled:true; maskSource:roundedMask; autoPaddingEnabled:false }
    }
    Rectangle {
        anchors.bottom:parent.bottom; width:parent.width; height:dock.shelfThickness; radius:22
        color:Theme.preferences.reduceTransparency?Theme.colors.background:Theme.compositorEffects ? (Theme.dark?"#b8323236":"#c8dedee3") : (Theme.dark?"#e6323236":"#e6dedee3")
        border.color:Theme.outline; border.width:1
    }
    Item {
        width: parent.width; anchors.bottom:parent.bottom; height:dock.shelfThickness
        Repeater {
            model:shell.referenceDock
            Item {
                id: tile
                required property var modelData
                required property int index
                x:12+(dock.position==="right"?shell.referenceDock.length-1-index:index)*dock.slot; width:dock.slot; height:dock.shelfThickness
                readonly property var appWindows: shell.dockWindows(modelData)
                Rectangle {visible:tile.modelData.separatorBefore===true || (tile.modelData.key==="downloads" && tile.modelData.separatorBefore===undefined);x:-5;y:15;width:1;height:42;color:Theme.outline}
                readonly property real magnification: !Theme.preferences.dockMagnification || dock.hovered<0 ? 0 : Math.max(0,1-Math.abs(index-dock.pointerSlot)*0.48)
                Rectangle {anchors.fill:parent;anchors.margins:2;radius:10;color:"transparent";border.width:2;border.color:Theme.colors.accent;visible:dock.keyboardMode && dock.keyboardIndex===tile.index}
                AppIcon {
                    id: icon
                    rotation:-surface.rotation;anchors.horizontalCenter:parent.horizontalCenter; anchors.bottom:parent.bottom; anchors.bottomMargin:13
                    width:tile.modelData.canvas*dock.artworkScale+18*tile.magnification; height:width
                    iconSource:(tile.modelData.key==="downloads" || tile.modelData.action==="folder") ? Qt.resolvedUrl("icons/folder.svg") : tile.modelData.action==="file"?Quickshell.iconPath("text-x-generic",true): tile.modelData.image ? Qt.resolvedUrl("reference-icons/"+tile.modelData.image) : Quickshell.iconPath(tile.modelData.entry.icon,true)
                    calendarDate:shell.currentDate

                    Behavior on width { NumberAnimation { duration:Theme.preferences.reduceMotion?0:150; easing.type:Easing.OutCubic } }
                }
                Rectangle {visible:tile.modelData.action==="minimized-window";x:icon.x+icon.width-18;y:icon.y+icon.height-16;width:18;height:14;radius:3;color:Theme.colors.background;border.color:Theme.colors.text;border.width:1;Accessible.ignored:true
                    Rectangle {x:2;y:3;width:14;height:1;color:Theme.colors.text}
                }
                Rectangle { anchors.horizontalCenter:parent.horizontalCenter; y:dock.shelfThickness-8; width:4; height:4; radius:2; color:Theme.colors.text; visible:tile.modelData.action!=="minimized-window" && Theme.preferences.dockIndicators!==false && (tile.appWindows.length>0 || (!!tile.modelData.entry && dock.applicationRunning(tile.modelData.entry))) }
                DropArea {anchors.fill:parent;keys:["text/uri-list"]
                    enabled:tile.modelData.action!=="minimized-window" && !!tile.modelData.entry && !dock.dragging
                    onEntered:drag=>{if(dropWorker.running){dock.refuseBusyDrop();drag.accepted=false;}else if(!drag.hasUrls || !(drag.supportedActions & Qt.CopyAction))drag.accepted=false;}
                    onDropped:drop=>{if(drop.hasUrls && tile.modelData.entry && dock.dropFiles(tile.modelData.entry,drop.urls))drop.accept(Qt.CopyAction);else drop.accepted=false;}
                }
                Accessible.role:Accessible.Button; Accessible.name:shell.dockLabel(modelData)
                Accessible.onPressAction:shell.activateDock(modelData,false,dock.outputName)
            }
        }
    }
    Rectangle {visible:dock.dragging && dock.hovered<0 && dock.removableDrag();anchors.horizontalCenter:parent.horizontalCenter;y:0;width:removeLabel.implicitWidth+20;height:removeLabel.implicitHeight+8;radius:10;color:Theme.colors.background;border.color:Theme.outline;border.width:1
        PanelLabel {id:removeLabel;anchors.centerIn:parent;text:"Remove from Dock";font.pixelSize:Theme.textSize(12);color:Theme.colors.text}
    }
    Rectangle {visible:dock.dragging && dock.hovered>=0;x:12+(dock.position==="right"?shell.referenceDock.length-1-dock.hovered:dock.hovered)*dock.slot;y:54;width:2;height:60;radius:1;color:Theme.colors.accent}
    PanelSurface {visible:!!(dock.dropError || shell.aliasError || shell.applicationCommandError);x:Math.max(0,(dock.width-width)/2);y:0;width:Math.min(420,dock.width);height:44
        PanelLabel {anchors.fill:parent;anchors.margins:8;text:shell.applicationCommandError || dock.dropError || shell.aliasError || "";wrapMode:Text.WordWrap;font.pixelSize:11;color:Theme.colors.text}
    }
    // A single stable hit area avoids pointer-grab and region transitions between tiles.
    MouseArea {
        id: pointerArea
        x: 0; y: 28; width: surface.width; height: surface.height-28
        hoverEnabled: true; acceptedButtons: Qt.LeftButton | Qt.RightButton
        onEntered:{dock.revealed=true;hideTimer.stop();dock.trackPointer(mouseX, mouseY);}
        onPositionChanged: mouse => {
            dock.trackPointer(mouse.x, mouse.y);
            if((mouse.buttons & Qt.LeftButton) && dock.pressedKey && Math.hypot(mouse.x-dock.pressX,mouse.y-dock.pressY)>12 && !shell.dockPreferencesBusy){
                const item=dock.dragIntentValid()?dock.pressIntent.item:null;
                if(item && ((item.action!=="minimized-window" && item.entry && shell.canConfigureDockEntry(item.entry)) || ["file","folder"].includes(item.action)) && !["files","applications","downloads","trash"].includes(item.key)){dock.dragging=true;keyboard.forceActiveFocus();}
            }
        }
        onExited:{dock.clearPointer();hideTimer.restart();}
        onCanceled: { dock.cancelDrag(); dock.clearPointer(); }
        onPressed: mouse => {
            dock.trackPointer(mouse.x,mouse.y);dock.pressX=mouse.x;dock.pressY=mouse.y;dock.dragCompleted=false;
            dock.pressedKey=dock.hovered>=0?shell.referenceDock[dock.hovered].key:"";
            dock.pressIntent=mouse.button===Qt.LeftButton && dock.hovered>=0 && !shell.dockPreferencesBusy?{items:shell.referenceDock,item:shell.referenceDock[dock.hovered]}:null;
        }
        onReleased: mouse => {
            if(dock.dragging){
                const source=dock.dragIntentValid()?dock.pressIntent.item:null;
                const target=shell.referenceDock[dock.hovered];
                if(source && !containsMouse){
                    if(["file","folder"].includes(source.action))shell.removeDockAlias(source);
                    else if(source.action!=="minimized-window" && source.entry && shell.canConfigureDockEntry(source.entry) && shell.isDockPinned(source.entry))shell.updateDockPin(source.entry,"remove");
                }else if(containsMouse && source && target && source.action!=="minimized-window" && target.action!=="minimized-window" && source.entry && target.entry && shell.isDockPinned(target.entry))shell.reorderDockPin(source.entry,target.entry);
                dock.cancelDrag();
            }
            if(!containsMouse){dock.pressIntent=null;dock.pressedKey="";dock.clearPointer();}
        }
        onClicked: mouse => {
            if(dock.dragCompleted){dock.dragCompleted=false;return;}
            dock.trackPointer(mouse.x, mouse.y);
            if (dock.hovered < 0) return;
            const index = dock.hovered;
            const item = DockInteraction.releaseTarget(shell.referenceDock,dock.pressedKey,index);
            dock.pressedKey="";dock.pressIntent=null;
            if (!item) return;
            if(item.action==="minimized-window" && (mouse.button===Qt.RightButton || (mouse.modifiers & Qt.ControlModifier)))return;
            if ((mouse.button === Qt.RightButton || (mouse.modifiers & Qt.ControlModifier)) && (item.entry || ["file","folder"].includes(item.action))) {
                dock.openContext(item,index);
            } else shell.activateDock(item, !!(mouse.modifiers & Qt.ShiftModifier), dock.outputName, mouse.modifiers);
        }
    }

    }
    PanelWindow {
        screen:dock.screen;visible:dock.displayAllowed && dock.hovered>=0 && (!dock.autoHide || dock.revealed) && !dock.dragging && !dock.contextEntry && !dock.contextAlias
        anchors {top:true;bottom:true;left:true;right:true}
        exclusionMode:ExclusionMode.Ignore;color:"transparent";mask:Region {}
        WlrLayershell.layer:WlrLayer.Overlay;WlrLayershell.namespace:"lumen-dock-tooltip"
        WlrLayershell.keyboardFocus:WlrKeyboardFocus.None
        Rectangle {
            width:Math.min(tip.implicitWidth+20,parent.width-16);height:26;radius:7;color:Theme.colors.background;border.color:Theme.colors.border
            x:Math.max(8,Math.min(parent.width-width-8,dock.vertical?(dock.position==="left"?dock.shelfThickness+24:parent.width-dock.shelfThickness-width-24):((parent.width-dock.width)/2+12+(dock.hovered+0.5)*dock.slot-width/2)))
            y:Math.max(8,Math.min(parent.height-height-8,dock.vertical?((parent.height-dock.height)/2+12+(dock.hovered+0.5)*dock.slot-height/2):parent.height-dock.shelfThickness-height-18))
            Text {id:tip;anchors.centerIn:parent;width:parent.width-20;elide:Text.ElideRight;text:dock.hovered>=0 && shell.referenceDock[dock.hovered]?shell.dockLabel(shell.referenceDock[dock.hovered]):"";color:Theme.colors.text;font.family:Theme.font;font.pixelSize:13}
        }
    }
    PanelWindow {
        screen:dock.screen; visible:dock.displayAllowed && (dock.contextEntry!==null || dock.contextAlias!==null)
        anchors {top:true;bottom:true;left:true;right:true}
        exclusionMode:ExclusionMode.Ignore; color:"transparent"
        WlrLayershell.layer:WlrLayer.Overlay; WlrLayershell.namespace:"lumen-dock-popup"
        WlrLayershell.keyboardFocus:WlrKeyboardFocus.Exclusive
        MouseArea {anchors.fill:parent;onClicked:{dock.contextEntry=null;dock.contextAlias=null;}}
        Item {anchors.fill:parent;focus:true
            Shortcut {sequence:"Escape";enabled:dock.contextEntry!==null || dock.contextAlias!==null;onActivated:dock.closeContext()}
            Shortcut {sequence:"Down";enabled:dock.contextEntry!==null || dock.contextAlias!==null;onActivated:dock.stepContext(1)}
            Shortcut {sequence:"Up";enabled:dock.contextEntry!==null || dock.contextAlias!==null;onActivated:dock.stepContext(-1)}
            Shortcut {sequence:"Return";enabled:dock.contextEntry!==null || dock.contextAlias!==null;onActivated:if(dock.contextSelection)dock.activateContext(dock.contextSelection.row,dock.contextSelection)}
        }
        PanelSurface {
            id:contextPanel
            Accessible.role:Accessible.PopupMenu;Accessible.name:"Dock options for "+(dock.contextEntry?dock.contextEntry.name:dock.contextAlias?dock.contextAlias.name:"item")
            FontMetrics {id:contextMetrics;font.family:Theme.font;font.pixelSize:13}
            x:dock.vertical?(dock.position==="left"?dock.shelfThickness+20:parent.width-width-dock.shelfThickness-20):Math.max(8,Math.min(parent.width-width-8,dock.contextX-width/2)); y:dock.vertical?Math.max(Theme.barHeight+8,Math.min(parent.height-height-8,dock.contextY-height/2)):Math.max(Theme.barHeight+8,parent.height-height-dock.shelfThickness-18)
            width:Math.max(220,Math.min(380,contextItems.model.reduce((w,n)=>Math.max(w,contextMetrics.advanceWidth(n.label)+36),0)));height:Math.min(contextColumn.height+16,parent.height-Theme.barHeight-108);radius:Theme.menuRadius;color:Theme.colors.background;border.color:Theme.outline
            MouseArea {anchors.fill:parent}
            Flickable {
                id:contextScroll;anchors.fill:parent;anchors.margins:8;contentWidth:width;contentHeight:contextColumn.height;clip:true;boundsBehavior:Flickable.StopAtBounds
            Column {
                id:contextColumn;spacing:2
                Repeater {
                    id:contextItems
                    model:dock.contextAlias?[{label:"Open",action:"alias-open"},{label:"Show in Files",action:"alias-reveal"},{label:"Remove from Dock",action:"alias-remove",enabled:!shell.dockPreferencesBusy}]:dock.contextEntry ? [{label:"Open New",action:"new"}].concat(shell.windowsFor(dock.contextEntry).map(w=>({label:(w.minimized?"Minimized — ":"")+(w.title || "Window"),action:"window",win:w}))).concat(!shell.canConfigureDockEntry(dock.contextEntry)?[]:shell.dockPreferencesBusy?[{label:"Saving Dock settings…",enabled:false}]:shell.isDockPinned(dock.contextEntry)?[{label:"Remove from Dock",action:"remove"},{label:"Move Earlier",action:"earlier"},{label:"Move Later",action:"later"}]:[{label:"Keep in Dock",action:"pin"}]).concat([{label:"Close Windows",action:"close",enabled:shell.windowsFor(dock.contextEntry).length>0},{label:"Request Quit",action:"quit",enabled:dock.applicationRunning(dock.contextEntry) && typeof shell.applicationInstanceKey==="function" && !!shell.applicationInstanceKey(dock.contextEntry)}]).concat(shell.canForceQuit(dock.contextEntry)?[{label:"Force Quit…",action:"force-quit"}]:[]) : []
                    AbstractButton {
                        id:contextButton;required property var modelData
                        required property int index
                        property var pressedContext:null
                        property int menuGeneration:0
                        Component.onCompleted:menuGeneration=dock.contextGeneration
                        width:contextPanel.width-16;height:Theme.menuRowHeight;enabled:modelData.enabled!==false
                        leftPadding:10;rightPadding:10;topPadding:0;bottomPadding:0;opacity:enabled?1:.45
                        Accessible.role:Accessible.MenuItem;Accessible.name:modelData.label;Accessible.selected:dock.contextSelected(modelData)
                        onActiveFocusChanged:if(activeFocus){if(dock.contextValid())dock.selectContext(index);else dock.closeContext();}
                        onHoveredChanged:if(hovered)dock.selectContext(index)
                        onPressed:pressedContext=dock.captureContext(modelData,menuGeneration)
                        onCanceled:pressedContext=null
                        onClicked:{const intent=pressedContext || dock.captureContext(modelData,menuGeneration);pressedContext=null;dock.activateContext(intent.row,intent);}
                        contentItem:Text {verticalAlignment:Text.AlignVCenter;textFormat:Text.PlainText;text:contextButton.modelData.label;elide:Text.ElideRight;color:dock.contextSelected(contextButton.modelData)?Theme.colors.selectedText:Theme.colors.text;font.family:Theme.font;font.pixelSize:13;Accessible.ignored:true}
                        background:Rectangle {radius:5;color:dock.contextSelected(contextButton.modelData)?Theme.colors.accent:"transparent"}
                    }
                }
            }
        }
    }
}
}

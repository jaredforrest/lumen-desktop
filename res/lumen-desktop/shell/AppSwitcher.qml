import QtQuick
import QtQuick.Controls
import QtQuick.Window
import Quickshell
import Quickshell.Wayland
import "AppSwitcherModel.js" as SwitcherModel

PanelWindow {
    id: root
    required property var shell
    visible:false
    anchors {top:true;bottom:true;left:true;right:true}
    exclusionMode:ExclusionMode.Ignore
    color:"transparent"
    WlrLayershell.namespace:"lumen-app-switcher"
    WlrLayershell.layer:WlrLayer.Overlay
    WlrLayershell.keyboardFocus:visible?WlrKeyboardFocus.Exclusive:WlrKeyboardFocus.None
    onVisibleChanged:if(!visible && shell.popupOwner==="app-switcher")shell.popupOwner=""
    property var items:[]
    // Measure the same Text layout as visible captions; fixed baseline size is intentional.
    Item {id:captionMeasures;visible:false
        Repeater {model:root.items
            Item {
                required property var modelData
                readonly property real advanceScale:baseline.implicitWidth>0?scaled.implicitWidth/baseline.implicitWidth:Theme.textScale
                Text {id:baseline;text:parent.modelData.label;textFormat:Text.PlainText;font.family:Theme.font;font.pixelSize:12}
                Text {id:scaled;text:parent.modelData.label;textFormat:Text.PlainText;font.family:Theme.font;font.pixelSize:Theme.textRole("label")}
            }
        }
    }
    readonly property real captionAdvanceScale:Array.from(captionMeasures.children).reduce((factor,item)=>Math.max(factor,Number(item.advanceScale) || Theme.textScale),Theme.textScale)
    readonly property int tileWidth:Math.max(104,Math.round(104*Theme.textScale),Math.ceil(96*captionAdvanceScale)+8)
    onTileWidthChanged:if(visible)Qt.callLater(()=>{if(root.visible && root.selected>=0){view.forceLayout();view.positionViewAtIndex(root.selected,ListView.Contain);}})
    property int selected:-1
    property string phase:"closed"
    property int commits:0
    readonly property bool keyboardReady:visible && keyboard.activeFocus && keyboard.Window.active
    property string pendingNativeRequest:""
    property var previewNativeFocus:null
    property var previewInventoryWitness:null
    function identity(window) {
        const entry=shell.entryFor(window);
        return entry?entry.id:window.appId;
    }
    function command(action) {
        if(action==="status")return;
        if(action==="cancel"){if(visible){visible=false;phase="cancelled";}return;}
        if(action==="commit"){commit();return;}
        if(!["begin","next","previous"].includes(action))return;
        if(!visible && (phase==="pending" || shell.nativeCommitBlocked))return;
        if(!visible){
            pendingNativeRequest="";
            const observation=SwitcherModel.nativeObservation(shell.nativeWindowObservation);
            previewNativeFocus=observation?observation.source:null;
            previewInventoryWitness=observation?observation.inventoryWitness:null;
            const windows=Array.from(shell.windows);
            items=SwitcherModel.snapshot(windows,shell.activationHistory,identity).map(item=>{
                const pairs=observation?SwitcherModel.nativeGroupPairs(windows,observation.captures,identity,item.key):null;
                const native=pairs && pairs.length?pairs[0]:null;
                const window=native?native.window:item.window;
                const entry=shell.entryFor(window);
                return {key:item.key,window:window,
                    nativeCapture:native?JSON.parse(JSON.stringify(native.capture)):null,
                    label:entry?shell.appLabel(entry):shell.nameForAppId(window.appId,window.title),
                    icon:entry?shell.appIcon(entry):Qt.resolvedUrl("icons/application.svg")};
            });
            const background=[];
            for(const app of shell.runningApplications || []){
                if(!app.zeroWindows)continue;
                const entry=shell.entryFor({appId:app.appId,title:""});
                if(!entry || items.some(item=>item.key===entry.id) || background.some(item=>item.key===entry.id))continue;
                background.push({key:entry.id,window:null,entry:entry,instanceKey:shell.applicationInstanceKey(entry),label:shell.appLabel(entry),icon:shell.appIcon(entry)});
            }
            items=SwitcherModel.applicationOrder(items.concat(background),shell.applicationActivationHistory || []);
            if(!items.length)return;
            const output=shell.preferredOutput();
            screen=Quickshell.screens.find(s=>s.name===output) || Quickshell.screens[0];
            shell.claimPopup("app-switcher");
            selected=action==="begin"?0:action==="previous"?items.length-1:Math.min(1,items.length-1);
            phase="preview";visible=true;
            Qt.callLater(()=>keyboard.forceActiveFocus());
        }else if(action!=="begin")selected=SwitcherModel.step(selected,items.length,action==="previous"?-1:1);
        view.positionViewAtIndex(selected,ListView.Contain);
    }
    function commit(){
        if(!visible)return;
        const captured=selected>=0 && selected<items.length?items[selected]:null;
        const background=captured && !captured.window && captured.entry && shell.runningFor(captured.entry) && captured.instanceKey===shell.applicationInstanceKey(captured.entry);
        const item=captured && captured.nativeCapture?captured:background?captured:SwitcherModel.validSelection(items,selected,shell.windows,identity);
        visible=false;phase=item?"committed":"cancelled";
        if(!item)return;
        const epoch=shell.focusEpoch;
        const initialFocus=JSON.parse(JSON.stringify(previewNativeFocus));
        const inventoryWitness=JSON.parse(JSON.stringify(previewInventoryWitness));
        phase="pending";
        // Allow the compositor to release the overlay keyboard grab first.
        Qt.callLater(()=>{
            if(epoch!==shell.focusEpoch){if(!visible)phase="cancelled";return;}
            if(item.nativeCapture){
                pendingNativeRequest=shell.focusCapturedNative(item.nativeCapture,initialFocus,inventoryWitness);
                if(!pendingNativeRequest)phase="cancelled";
                return;
            }
            const live=shell.windows.filter(w=>identity(w)===item.key);
            if(!live.length){
                if(item.entry && shell.runningFor(item.entry) && item.instanceKey===shell.applicationInstanceKey(item.entry)){++commits;phase="committed";shell.activate(item.entry,false);}
                else phase="cancelled";
                return;
            }
            const entry=shell.entryFor(live[0]);
            ++commits;phase="committed";
            if(entry)shell.activate(entry,false);
            else shell.focusWindow(live.includes(item.window)?item.window:live[0]);
        });
    }
    function status(){return JSON.stringify({visible:visible,keyboardReady:keyboardReady,phase:phase,selected:selected,key:items[selected]?items[selected].key:"",items:items.map(i=>i.key),nativeCapture:items[selected]?items[selected].nativeCapture || null:null,commits:commits});}
    Connections {target:shell
        function onNativeFocusResult(result){if(root.pendingNativeRequest && result.requestId===root.pendingNativeRequest){root.pendingNativeRequest="";root.phase=result.requested?"committed":"cancelled";if(result.requested)++root.commits;}}
        function onDismissPopups(owner){if(owner!=="app-switcher" && root.visible){root.visible=false;root.phase="cancelled";}}
    }
    Item {
        id:keyboard;anchors.fill:parent;focus:root.visible
        Keys.onPressed:event=>{
            if(event.key===Qt.Key_Escape || event.key===Qt.Key_Period){root.command("cancel");event.accepted=true;}
            else if(event.key===Qt.Key_Tab || event.key===Qt.Key_Backtab){root.command((event.modifiers & Qt.ShiftModifier)?"previous":"next");event.accepted=true;}
            else if(event.key===Qt.Key_Left || event.key===Qt.Key_Right){root.command(event.key===Qt.Key_Left?"previous":"next");event.accepted=true;}
            else if(event.key===Qt.Key_Return || event.key===Qt.Key_Enter){root.commit();event.accepted=true;}
            else event.accepted=true;
        }
        Keys.onReleased:event=>{
            if([Qt.Key_Meta,Qt.Key_Super_L,Qt.Key_Super_R].includes(event.key)){root.commit();event.accepted=true;}
        }
        MouseArea {anchors.fill:parent;onClicked:root.command("cancel")}
        Rectangle {
            anchors.centerIn:parent
            width:Math.min((root.screen?root.screen.width:1000)-64,root.items.length*root.tileWidth+32)
            height:Math.max(142,118+Math.ceil(Theme.textSize(12)*1.5));radius:20;color:Theme.colors.background;border.color:Theme.outline
            ListView {
                id:view;anchors.fill:parent;anchors.margins:16;orientation:ListView.Horizontal
                model:root.items;clip:true;spacing:0
                delegate:AbstractButton {
                    required property var modelData
                    required property int index
                    width:root.tileWidth;height:Math.max(110,86+Math.ceil(Theme.textSize(12)*1.5))
                    Accessible.name:modelData.label
                    Accessible.selected:index===root.selected
                    onClicked:{root.selected=index;root.commit();}
                    background:Rectangle {radius:12;color:parent.index===root.selected?Theme.colors.surface:"transparent";border.width:parent.index===root.selected?2:0;border.color:Theme.colors.accent}
                    contentItem:Item {
                        AppIcon {anchors.horizontalCenter:parent.horizontalCenter;y:8;width:66;height:66;iconSource:modelData.icon}
                        Text {anchors.bottom:parent.bottom;anchors.bottomMargin:6;width:parent.width-8;anchors.horizontalCenter:parent.horizontalCenter;horizontalAlignment:Text.AlignHCenter;elide:Text.ElideRight;text:parent.parent.Accessible.name;textFormat:Text.PlainText;color:Theme.colors.text;font.family:Theme.font;font.pixelSize:Theme.textRole("label")}
                    }
                }
            }
        }
    }
}

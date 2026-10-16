import QtQuick
import QtQuick.Controls
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
    property int selected:-1
    property string phase:"closed"
    property int commits:0
    function identity(window) {
        const entry=shell.entryFor(window);
        return entry?entry.id:window.appId;
    }
    function command(action) {
        if(action==="status")return;
        if(action==="cancel"){if(visible){visible=false;phase="cancelled";}return;}
        if(action==="commit"){commit();return;}
        if(!["begin","next","previous"].includes(action))return;
        if(!visible){
            items=SwitcherModel.snapshot(shell.windows,shell.activationHistory,identity).map(item=>{
                const entry=shell.entryFor(item.window);
                return {key:item.key,window:item.window,
                    label:entry?shell.appLabel(entry):shell.nameForAppId(item.window.appId,item.window.title),
                    icon:entry?shell.appIcon(entry):Qt.resolvedUrl("icons/application.svg")};
            });
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
        const item=SwitcherModel.validSelection(items,selected,shell.windows,identity);
        visible=false;phase=item?"committed":"cancelled";
        if(!item)return;
        const epoch=shell.focusEpoch;
        phase="pending";
        // Allow the compositor to release the overlay keyboard grab first.
        Qt.callLater(()=>{
            const live=shell.windows.filter(w=>identity(w)===item.key);
            if(epoch!==shell.focusEpoch){if(!visible)phase="cancelled";return;}
            if(!live.length){phase="cancelled";return;}
            const entry=shell.entryFor(live[0]);
            ++commits;phase="committed";
            if(entry)shell.activate(entry,false);
            else shell.focusWindow(live.includes(item.window)?item.window:live[0]);
        });
    }
    function status(){return JSON.stringify({visible:visible,phase:phase,selected:selected,key:items[selected]?items[selected].key:"",items:items.map(i=>i.key),commits:commits});}
    Connections {target:shell;function onDismissPopups(owner){if(owner!=="app-switcher" && root.visible){root.visible=false;root.phase="cancelled";}}}
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
            width:Math.min((root.screen?root.screen.width:1000)-64,root.items.length*104+32)
            height:142;radius:20;color:Theme.colors.background;border.color:Theme.outline
            ListView {
                id:view;anchors.fill:parent;anchors.margins:16;orientation:ListView.Horizontal
                model:root.items;clip:true;spacing:0
                delegate:AbstractButton {
                    required property var modelData
                    required property int index
                    width:104;height:110
                    Accessible.name:modelData.label
                    Accessible.selected:index===root.selected
                    onClicked:{root.selected=index;root.commit();}
                    background:Rectangle {radius:12;color:parent.index===root.selected?Theme.colors.surface:"transparent";border.width:parent.index===root.selected?2:0;border.color:Theme.colors.accent}
                    contentItem:Item {
                        AppIcon {anchors.horizontalCenter:parent.horizontalCenter;y:8;width:66;height:66;iconSource:modelData.icon}
                        Text {anchors.bottom:parent.bottom;anchors.bottomMargin:6;width:parent.width-8;anchors.horizontalCenter:parent.horizontalCenter;horizontalAlignment:Text.AlignHCenter;elide:Text.ElideRight;text:parent.parent.Accessible.name;textFormat:Text.PlainText;color:Theme.colors.text;font.family:Theme.font;font.pixelSize:12}
                    }
                }
            }
        }
    }
}

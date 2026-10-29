import QtQuick
import Quickshell
import Quickshell.Io
import "assets/shell" as D
import "assets/shell/DockModel.js" as Model
ShellRoot {
    id:audit
    property var pointerEvents:[]
    function observe(kind,buttons,x,y){pointerEvents=pointerEvents.concat([{kind:kind,buttons:buttons,x:x,y:y,key:dock.pressedKey,valid:!!dock.dragIntentValid(),dragging:dock.dragging,busy:stub.dockPreferencesBusy,tip:dock.fixtureTipWindow.visible,tipShown:dock.fixtureTipLifecycle.shown,tipRetired:dock.fixtureTipLifecycle.retired,tipEligible:dock.fixtureTipLifecycle.eligible,intent:dock.pressIntent?dock.pressIntent.item.key:""}]).slice(-64);}
    Connections {target:dock.fixturePointer;function onPressed(mouse){audit.observe("press",mouse.buttons,mouse.x,mouse.y);}function onPositionChanged(mouse){audit.observe("motion",mouse.buttons,mouse.x,mouse.y);}function onCanceled(){audit.observe("cancel",0,0,0);}}
    Connections {target:dock.fixtureTipLifecycle;function onShownChanged(){audit.observe("tooltip",0,0,0);}function onEligibleChanged(){audit.observe("tooltip-eligibility",0,0,0);}}
    Connections {target:stub;function onReferenceDockChanged(){audit.observe("model",0,0,0);}}
    D.SettingsState { id: settings; active: true }
    QtObject {
        id: stub
        property var apps: [
            {id:"fixture-a",name:"Fixture A",icon:"application-x-executable"},
            {id:"fixture-b",name:"Fixture B",icon:"application-x-executable"},
            {id:"fixture-c",name:"Fixture C",icon:"application-x-executable"},
            {id:"fixture-recent",name:"Recent",icon:"application-x-executable"}
        ]
        property var reference: [
            {key:"files",action:"files",image:"files.svg",canvas:68},
            {key:"a",appId:"fixture-a",canvas:68},
            {key:"b",appId:"fixture-b",canvas:68},
            {key:"c",appId:"fixture-c",canvas:68},
            {key:"downloads",action:"downloads",image:"files.svg",canvas:68},
            {key:"trash",action:"trash",image:"files.svg",canvas:68}
        ]
        property bool dockPreferencesBusy: settings.busy
        property var referenceDock: Model.build(reference,apps,[],settings.preferences.dockPins,[apps[3]],true,settings.preferences.dockFolders,settings.preferences.dockFiles)
        property string popupOwner: ""
        property date currentDate: new Date()
        property var activations: []
        property string aliasError: ""
        property string applicationCommandError: ""
        signal requestDockFocus(string name)
        signal dismissPopups(string owner)
        function claimPopup(owner) { popupOwner=owner; dismissPopups(owner); }
        function dockWindows(item) { return []; }
        function dockLabel(item) { return item.key; }
        function runningFor(entry) { return false; }
        function activateDock(item,shift,name) { activations=activations.concat(item.key); }
        function canConfigureDockEntry(entry) { return entry.id!=="lumen-files"; }
        function isDockPinned(entry) { return (settings.preferences.dockPins || []).includes(entry.id); }
        function updateDockPin(entry,action) { throw Error("Unexpected pin removal"); }
        function reorderDockPin(source,target) { if (!dockPreferencesBusy) settings.setPreference("dockPins",Model.movePin(settings.preferences.dockPins,source.id,target.id)); }
        function removeDockAlias(item) { throw Error("Unexpected alias removal"); }
    }
    D.Dock { id: dock; shell: stub; screen: Quickshell.screens[0] }
    IpcHandler {
        target: "dockAudit"
        function status(): string {
            return JSON.stringify({keys:stub.referenceDock.map(x=>x.key),separators:stub.referenceDock.map(x=>x.separatorBefore),
                width:dock.width,height:dock.height,slot:dock.slot,shelf:dock.shelfThickness,
                pressedKey:dock.pressedKey,pressIntent:dock.pressIntent?dock.pressIntent.item.key:"",intentValid:!!dock.dragIntentValid(),pointerEvents:audit.pointerEvents,dragging:dock.dragging,hovered:dock.hovered,pointerSlot:dock.pointerSlot,
                contains:dock.fixturePointer.containsMouse,pressed:dock.fixturePointer.pressed,
                tip:dock.fixtureTip.text,tipVisible:dock.fixtureTipWindow.visible,tiles:dock.fixtureTiles(),
                activations:stub.activations,busy:settings.busy,ready:settings.available,pins:settings.preferences.dockPins});
        }
    }
}

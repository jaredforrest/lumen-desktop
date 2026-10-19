import QtQuick
import QtQuick.Window
import Quickshell
FloatingWindow {
    id:root
    title:"File Info"+(owner && owner.windowNumber>1?" "+owner.windowNumber:"");visible:false;color:"transparent"
    implicitWidth:360;implicitHeight:500;minimumSize:Qt.size(280,300)
    property var owner:null
    readonly property bool keyActive:body.Window.window?body.Window.window.active:false
    onKeyActiveChanged:{if(!owner || !owner.shell.ownedContext)return;if(keyActive){owner.shell.ownedContext.focus(root,owner,"lumen-files","utility");owner.shell.activeFiles(owner);}else owner.shell.ownedContext.leave(root);}
    onVisibleChanged:if(!visible && owner && owner.shell.ownedContext)owner.shell.ownedContext.close(root)
    Component.onDestruction:if(owner && owner.shell.ownedContext)owner.shell.ownedContext.close(root)
    property var details:({})
    function show(value){if(owner && owner.shell.claimPopup)owner.shell.claimPopup("");details=value;visible=true;if(owner)owner.shell.focusOwnedWindow(root.title);Qt.callLater(()=>{if(body.Window.window)body.Window.window.requestActivate();});}
    Rectangle {id:body;anchors.fill:parent;color:Theme.colors.background;radius:Theme.nativeWindowFrame?0:12
        MouseArea {visible:!Theme.nativeWindowFrame;width:parent.width;height:48;onPressed:body.Window.window.startSystemMove()}
        WindowControls {windowName:root.title;canMinimize:false;canFullscreen:false;windowActive:body.Window.window?body.Window.window.active:false;onTriggered:root.visible=false}
        PanelLabel {x:100;y:14;visible:!Theme.nativeWindowFrame;width:parent.width-112;text:"Info";font.weight:Font.DemiBold}
        FileInfoPanel {x:16;y:Theme.nativeWindowFrame?14:58;width:parent.width-32;height:parent.height-y-16;details:root.details}
        Shortcut {sequence:"Escape";enabled:root.visible;onActivated:root.visible=false}
    }
}

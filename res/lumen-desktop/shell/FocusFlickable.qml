import QtQuick
import QtQuick.Window
import "ScrollGeometry.js" as ScrollGeometry
Flickable {
    id:root
    function revealFocus(){
        if(!root.visible || !root.Window.window)return;
        const item=root.Window.window.activeFocusItem;
        let ancestor=item;
        while(ancestor && ancestor!==root.contentItem)ancestor=ancestor.parent;
        if(!ancestor || !item)return;
        const at=item.mapToItem(root.contentItem,0,0);
        root.contentY=ScrollGeometry.reveal(root.contentY,root.height,root.contentHeight,at.y,item.height,8);
    }
    Connections {target:root.Window.window;function onActiveFocusItemChanged(){Qt.callLater(root.revealFocus);}}
    onHeightChanged:Qt.callLater(root.revealFocus)
    onContentHeightChanged:Qt.callLater(root.revealFocus)
}

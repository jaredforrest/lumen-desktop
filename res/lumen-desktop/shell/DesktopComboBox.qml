import QtQuick
import QtQuick.Controls
import QtQuick.Window
ComboBox {
    id:root
    onCountChanged:if(count>0 && currentIndex<0)currentIndex=0
    implicitHeight:30
    font.family:Theme.font
    font.pixelSize:12
    leftPadding:10;rightPadding:28
    contentItem:PanelLabel {text:root.displayText;verticalAlignment:Text.AlignVCenter;elide:Text.ElideRight;opacity:root.enabled?1:0.45}
    indicator:PanelLabel {x:root.width-22;y:(root.height-height)/2;text:"⌄";font.pixelSize:15}
    background:Rectangle {radius:6;color:root.down?Theme.colors.hover:Theme.colors.field;border.color:root.activeFocus?Theme.colors.accent:Theme.hairline}
    popup:Popup {
        y: root.mapToItem(null,0,root.height).y+height+4 > root.Window.height ? -height-4 : root.height+4; width:root.width;implicitHeight:Math.min(300,choices.contentHeight+12);padding:6
        background:Rectangle {radius:9;color:Theme.colors.background;border.color:Theme.outline}
        contentItem:ListView {id:choices;clip:true;implicitHeight:contentHeight;model:root.popup.visible?root.delegateModel:null;currentIndex:root.highlightedIndex;ScrollBar.vertical:DesktopScrollBar {}}
    }
    delegate:ItemDelegate {
        width:root.width-12;height:30
        text:root.textRole?modelData[root.textRole]:modelData
        font:root.font
        highlighted:root.highlightedIndex===index
        contentItem:PanelLabel {text:(index===root.currentIndex?"✓  ":"    ")+parent.text;verticalAlignment:Text.AlignVCenter;color:parent.highlighted?Theme.colors.selectedText:Theme.colors.text}
        background:Rectangle {radius:5;color:parent.highlighted?Theme.colors.accent:"transparent"}
    }
}

import QtQuick
import QtQuick.Controls
Flickable {
    id: root
    property bool selectionActive:true
    property var columns: []
    required property var dropCoordinator
    property string currentPath: ""
    signal chosen(string path, bool directory)
    clip: true; contentWidth: columnRow.width; contentHeight: height
    onContentWidthChanged: Qt.callLater(()=>contentX=Math.max(0,contentWidth-width))
    ScrollBar.horizontal: DesktopScrollBar {}
    Row {
        id: columnRow; height: root.height
        Repeater {
            model: root.columns
            Item {
                required property var modelData; required property int index
                width: Math.max(200,Math.min(260,root.width/2)); height: columnRow.height
                Rectangle {anchors.right:parent.right;width:1;height:parent.height;color:Theme.hairline}
                PanelLabel {x:10;y:3;width:parent.width-20;height:24;text:modelData.name;textFormat:Text.PlainText;font.weight:Font.DemiBold;font.pixelSize:11;color:Theme.colors.muted}
                ListView {
                    anchors {top:parent.top;topMargin:30;left:parent.left;right:parent.right;bottom:parent.bottom;bottomMargin:12}
                    clip:true; model:parent.modelData.rows; ScrollBar.vertical:DesktopScrollBar {}
                    delegate:AbstractButton {
                        required property var modelData
                        readonly property bool selected:modelData.path===root.currentPath || root.columns.some(c=>c.path===modelData.path)
                        width:ListView.view.width-6;height:28
                        Accessible.name:modelData.name+(modelData.directory?", folder":"");Accessible.selected:selected
                        background:Rectangle {radius:5;color:parent.selected?(root.selectionActive?Theme.colors.accent:Theme.selection):parent.hovered?Theme.colors.hover:"transparent";border.color:parent.activeFocus?Theme.colors.accent:"transparent"}
                        contentItem:Item {
                            SystemGlyph {x:7;y:6;width:16;height:16;name:modelData.directory?"folder":"document";ink:parent.parent.selected && root.selectionActive?Theme.colors.selectedText:Theme.colors.accent}
                            PanelLabel {x:29;width:parent.width-48;height:parent.height;verticalAlignment:Text.AlignVCenter;text:modelData.name;color:parent.parent.selected && root.selectionActive?Theme.colors.selectedText:Theme.colors.text;textFormat:Text.PlainText;font.pixelSize:12}
                            PanelLabel {visible:modelData.directory;anchors.right:parent.right;anchors.rightMargin:6;height:parent.height;verticalAlignment:Text.AlignVCenter;text:"›";color:Theme.colors.muted}
                        }
                        FolderDropTarget {anchors.fill:parent;enabled:modelData.directory;folder:modelData.path;coordinator:root.dropCoordinator}
                        onClicked:root.chosen(modelData.path,modelData.directory)
                    }
                }
            }
        }
    }
}

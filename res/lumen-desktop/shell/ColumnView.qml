import QtQuick
import QtQuick.Controls
import "FilesHistoryModel.js" as HistoryModel
import "FileLinks.js" as FileLinks
Flickable {
    id: root
    property bool selectionActive:true
    property var columns: []
    required property var dropCoordinator
    property string currentPath: ""
    signal chosen(string path, bool directory)
    clip: true; contentWidth: columnRow.width; contentHeight: height
    property var pendingSnapshot:null
    function snapshot(){return {x:contentX,columns:columns.map((column,index)=>{const item=columnRepeater.itemAt(index);if(!item)return {path:column.path,offset:0};const list=item.listView;const row=list.indexAt(1,list.contentY+1);const delegate=row>=0?list.itemAtIndex(row):null;return {path:column.path,offset:list.contentY,anchor:HistoryModel.ref(column.rows[row]),delta:delegate?Math.max(0,list.contentY-delegate.y):0};})};}
    function ensureVisible(path){columns.forEach((column,index)=>{const item=columnRepeater.itemAt(index),row=column.rows.findIndex(r=>r.path===path);if(item && row>=0){item.listView.positionViewAtIndex(row,ListView.Contain);contentX=Math.max(0,Math.min(item.x,contentWidth-width));}});}
    function restoreSnapshot(value){pendingSnapshot=value;Qt.callLater(applySnapshot);}
    function applySnapshot(){
        if(!pendingSnapshot)return;
        contentX=Math.max(0,Math.min(pendingSnapshot.x || 0,contentWidth-width));
        columns.forEach((column,index)=>{const saved=(pendingSnapshot.columns || []).find(c=>c.path===column.path),item=columnRepeater.itemAt(index);if(!saved || !item || column.loaded===false)return;const list=item.listView,i=HistoryModel.locate(column.rows,saved.anchor);if(i>=0)list.positionViewAtIndex(i,ListView.Beginning);list.contentY=Math.max(0,Math.min(i>=0?list.contentY+(saved.delta || 0):saved.offset || 0,list.contentHeight-list.height));});
        if(columns.every(column=>column.loaded!==false))pendingSnapshot=null;
    }
    onColumnsChanged:Qt.callLater(applySnapshot)
    onContentWidthChanged: Qt.callLater(()=>{if(pendingSnapshot)applySnapshot();else contentX=Math.max(0,contentWidth-width);})
    ScrollBar.horizontal: DesktopScrollBar {}
    Row {
        id: columnRow; height: root.height
        Repeater {
            id:columnRepeater
            model: root.columns
            Item {
                required property var modelData; required property int index
                property alias listView:columnList
                width: Math.max(200,Math.min(260,root.width/2)); height: columnRow.height
                Rectangle {anchors.right:parent.right;width:1;height:parent.height;color:Theme.hairline}
                PanelLabel {x:10;y:3;width:parent.width-20;height:24;text:modelData.name;textFormat:Text.PlainText;font.weight:Font.DemiBold;font.pixelSize:11;color:Theme.colors.muted}
                ListView {
                    id:columnList;objectName:"files-column-"+parent.modelData.path
                    anchors {top:parent.top;topMargin:30;left:parent.left;right:parent.right;bottom:parent.bottom;bottomMargin:12}
                    clip:true; model:parent.modelData.rows; ScrollBar.vertical:DesktopScrollBar {}
                    delegate:AbstractButton {
                        required property var modelData
                        readonly property bool selected:modelData.path===root.currentPath || root.columns.some(c=>c.path===modelData.path)
                        width:ListView.view.width-6;height:28
                        Accessible.name:FileLinks.accessible(modelData)+(modelData.directory?", folder":"");Accessible.selected:selected
                        background:Rectangle {radius:5;color:parent.selected?(root.selectionActive?Theme.colors.accent:Theme.selection):parent.hovered?Theme.colors.hover:"transparent";border.color:parent.activeFocus?Theme.colors.accent:"transparent"}
                        contentItem:Item {
                            SystemGlyph {x:7;y:6;width:16;height:16;name:modelData.directory?"folder":"document";ink:parent.parent.selected && root.selectionActive?Theme.colors.selectedText:Theme.colors.accent}
                            PanelLabel {x:29;width:parent.width-48;height:parent.height;verticalAlignment:Text.AlignVCenter;text:FileLinks.markedLabel(modelData);color:parent.parent.selected && root.selectionActive?Theme.colors.selectedText:Theme.colors.text;textFormat:Text.PlainText;font.pixelSize:12}
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

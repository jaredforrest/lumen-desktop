import QtQuick
import QtQuick.Controls
import "FilesHistoryModel.js" as HistoryModel
import "FileLayoutModel.js" as LayoutModel
import "FileLinks.js" as FileLinks
import "FilesSelectionModel.js" as SelectionModel
Flickable {
    id: root
    property bool selectionActive:true
    property var preferredWidths:[]
    signal widthCommitted(int index,int value)
    property var columns: []
    required property var dropCoordinator
    property string currentPath: ""
    property var selectedPaths:[]
    property var bandBasePaths:[]
    property var bandRows:[]
    property bool bandPermitted:false
    property string bandContext:""
    property int bandCurrent:-1
    property int bandAnchor:-1
    property string selectionFolder:""
    signal bandBegan()
    signal bandPreviewed(var paths)
    signal bandCommitted(var selection)
    signal bandCanceled(string reason)
    signal chosen(string path, bool directory)
    clip: true; contentWidth: columnRow.width; contentHeight: height
    property int resizingIndex:-1
    property int resizeEpoch:0
    property real resizeOriginX:0
    readonly property string columnPathKey:columns.map(column=>column.path).join("\u0000")
    function clampOffset(value){return Math.max(0,Math.min(value,Math.max(0,contentWidth-width)));}
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
    onColumnPathKeyChanged:{resizeEpoch++;resizingIndex=-1;Qt.callLater(()=>{if(pendingSnapshot)applySnapshot();else if(resizingIndex<0)contentX=Math.max(0,contentWidth-width);});}
    onContentWidthChanged:Qt.callLater(()=>{if(pendingSnapshot)applySnapshot();else contentX=clampOffset(contentX);})
    ScrollBar.horizontal: DesktopScrollBar {}
    Row {
        id: columnRow; height: root.height
        Repeater {
            id:columnRepeater
            model: root.columns
            Item {
                required property var modelData; required property int index
                property alias listView:columnList
                id:columnPane
                property int dragWidth:-1
                width:LayoutModel.columnWidth(dragWidth>=0?dragWidth:root.preferredWidths[index],root.width,Theme.textScale);height:columnRow.height
                SplitResizeHandle {objectName:"files-column-resize-"+columnPane.index;label:"Column "+(columnPane.index+1)+" width";x:parent.width-width;height:parent.height;z:5;value:columnPane.width;minimum:Math.ceil(200*Theme.textScale);maximum:Math.max(minimum,Math.min(600,Math.floor(root.width)));visible:maximum>minimum
                    onGestureBegan:{root.resizeEpoch++;root.resizingIndex=columnPane.index;root.resizeOriginX=root.contentX;}
                    onPreviewRequested:value=>columnPane.dragWidth=value
                    onCommitted:value=>{root.widthCommitted(columnPane.index,value);columnPane.dragWidth=-1;root.resizingIndex=-1;}
                    onCancelled:{const active=root.resizingIndex===columnPane.index,epoch=root.resizeEpoch,origin=root.resizeOriginX;columnPane.dragWidth=-1;if(active){root.resizingIndex=-1;Qt.callLater(()=>{if(root.resizeEpoch===epoch && root.resizingIndex<0)root.contentX=root.clampOffset(origin);});}}
                }
                PanelLabel {x:10;y:3;width:parent.width-20;height:24;text:modelData.name;textFormat:Text.PlainText;font.weight:Font.DemiBold;font.pixelSize:Theme.textRole("metadata");color:Theme.colors.muted}
                FilesBandSelection {
                    objectName:"files-column-band-"+columnList.parent.modelData.path
                    parent:columnList;anchors.fill:parent;view:columnList;rows:root.bandRows
                    selectedPaths:root.bandBasePaths;currentIndex:root.bandCurrent;anchorIndex:root.bandAnchor
                    contextKey:root.bandContext+"|"+root.contentX
                    permitted:root.bandPermitted && columnList.parent.modelData.path===root.selectionFolder && columnList.parent.modelData.loaded!==false && SelectionModel.revision(root.bandRows)===SelectionModel.revision(columnList.model)
                    rightInset:columnScrollBar.visible?columnScrollBar.width:0
                    onBegan:root.bandBegan()
                    onPreviewed:paths=>root.bandPreviewed(paths)
                    onCommitted:value=>root.bandCommitted(value)
                    onCanceled:reason=>root.bandCanceled(reason)
                }
                ListView {
                    id:columnList;objectName:"files-column-"+parent.modelData.path
                    anchors {top:parent.top;topMargin:30;left:parent.left;right:parent.right;rightMargin:8;bottom:parent.bottom;bottomMargin:12}
                    clip:true; model:parent.modelData.rows; ScrollBar.vertical:DesktopScrollBar {id:columnScrollBar}
                    delegate:AbstractButton {
                        required property var modelData
                        readonly property bool selected:ListView.view.parent.modelData.path===root.selectionFolder?root.selectedPaths.includes(modelData.path):modelData.path===root.currentPath || root.columns.some(c=>c.path===modelData.path)
                        width:Math.max(0,ListView.view.width-12-(columnScrollBar.visible?columnScrollBar.width:0));height:28+(Theme.textRole("label")-12)
                        Accessible.name:FileLinks.accessible(modelData)+(modelData.directory?", folder":"");Accessible.selected:selected
                        background:Rectangle {radius:5;color:parent.selected?(root.selectionActive?Theme.colors.accent:Theme.selection):parent.hovered?Theme.colors.hover:"transparent";border.color:parent.activeFocus?Theme.colors.accent:"transparent"}
                        contentItem:Item {
                            SystemGlyph {x:7;y:6;width:16;height:16;name:modelData.directory?"folder":"document";ink:parent.parent.selected && root.selectionActive?Theme.colors.selectedText:Theme.colors.accent}
                            PanelLabel {x:29;width:parent.width-48;height:parent.height;verticalAlignment:Text.AlignVCenter;text:FileLinks.markedLabel(modelData);color:parent.parent.selected && root.selectionActive?Theme.colors.selectedText:Theme.colors.text;textFormat:Text.PlainText;font.pixelSize:Theme.textRole("label")}
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

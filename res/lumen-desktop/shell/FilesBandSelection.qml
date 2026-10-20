import QtQuick
import "FilesSelectionModel.js" as Selection
Item {
    id:root
    required property var view
    required property var rows
    required property var selectedPaths
    required property int currentIndex
    required property int anchorIndex
    required property string contextKey
    property bool permitted:false
    property real rightInset:0
    property real bottomInset:0
    // An engineering gesture threshold; no reference-device measurement claim.
    property real threshold:8
    property var transaction:null
    property var start:({x:0,y:0})
    property var last:({x:0,y:0})
    property var band:null
    property bool pending:false
    property bool active:false
    property var hitRows:[]
    property var capturedBoxes:[]
    readonly property string rowsRevision:Selection.revision(rows)
    signal began()
    signal previewed(var paths)
    signal committed(var selection)
    signal canceled(string reason)
    z:5
    function cancel(reason){
        if(!pending)return;
        transaction=null;pending=false;active=false;band=null;hitRows=[];capturedBoxes=[];previewed(null);canceled(reason || "invalidate");
    }
    function boxes(){
        const result=[];
        for(let index=0;index<rows.length;index++){
            const item=view.itemAtIndex(index);
            if(item)result.push({row:rows[index],x:item.x,y:item.y,width:item.width,height:item.height});
        }
        return result;
    }
    function update(x,y){
        if(!pending)return;
        if(!transaction || transaction.context!==contextKey || transaction.revision!==rowsRevision){cancel();return;}
        last={x:Math.max(0,Math.min(x,area.width))+view.contentX,y:Math.max(0,Math.min(y,area.height))+view.contentY};
        if(!active && Math.hypot(last.x-start.x,last.y-start.y)<threshold)return;
        active=true;band=Selection.rectangle(start,last);hitRows=Selection.hits(band,capturedBoxes);
        const preview=Selection.combine(transaction.snapshot,hitRows,transaction.mode);
        previewed(preview?Selection.reconcile(rows,preview).paths:null);
    }
    onRowsChanged:if(pending && transaction && transaction.revision!==Selection.revision(rows))cancel()
    onContextKeyChanged:cancel()
    onSelectedPathsChanged:cancel()
    onCurrentIndexChanged:cancel()
    onAnchorIndexChanged:cancel()
    onPermittedChanged:if(!permitted)cancel()
    onVisibleChanged:if(!visible)cancel()
    onActiveFocusChanged:if(!activeFocus && pending)cancel("focus")
    Keys.onEscapePressed:event=>{if(pending){root.cancel("escape");event.accepted=true;}else event.accepted=false;}
    Connections {target:root.view
        function onContentXChanged(){root.cancel();}
        function onContentYChanged(){root.cancel();}
        function onWidthChanged(){root.cancel();}
        function onHeightChanged(){root.cancel();}
    }
    Rectangle {
        visible:root.active && !!root.band
        x:root.band?root.band.x-root.view.contentX:0;y:root.band?root.band.y-root.view.contentY:0
        width:root.band?root.band.width:0;height:root.band?root.band.height:0
        color:Theme.selection;border.color:Theme.colors.accent;border.width:1;opacity:0.65
    }
    MouseArea {
        id:area;width:Math.max(0,root.width-root.rightInset);height:Math.max(0,root.height-root.bottomInset)
        enabled:root.permitted;acceptedButtons:Qt.LeftButton;preventStealing:true
        onPressed:event=>{
            if(root.view.indexAt(event.x+root.view.contentX,event.y+root.view.contentY)>=0){event.accepted=false;return;}
            const mode=(event.modifiers & Qt.ControlModifier)?"toggle":(event.modifiers & Qt.ShiftModifier)?"add":"replace";
            const captured=Selection.begin(root.rows,root.selectedPaths,root.currentIndex,root.anchorIndex,root.contextKey,mode);
            if(!captured){event.accepted=false;return;}
            root.transaction=captured;root.capturedBoxes=root.boxes();root.start={x:event.x+root.view.contentX,y:event.y+root.view.contentY};root.last=root.start;root.pending=true;root.active=false;root.began();root.forceActiveFocus();
        }
        onPositionChanged:event=>root.update(event.x,event.y)
        onReleased:event=>{
            if(!root.pending)return;
            root.update(event.x,event.y);
            if(!root.pending)return;
            const result=Selection.finish(root.transaction,root.rows,root.contextKey,root.active?root.hitRows:[]);
            root.transaction=null;root.pending=false;root.active=false;root.band=null;root.hitRows=[];root.capturedBoxes=[];root.previewed(null);
            if(result)root.committed(result);else root.canceled("stale");
        }
        onCanceled:root.cancel("ungrab")
        onWheel:event=>{root.cancel("scroll");event.accepted=false;}
    }
}

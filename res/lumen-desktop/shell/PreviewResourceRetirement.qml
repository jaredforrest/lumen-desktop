import QtQuick
import QtMultimedia
Item {
    id:root
    property var consumers:[]
    property var pending:[]
    readonly property bool busy:pending.length>0
    signal retired(var requestIds)
    function source(object){const value=object && object.source!==undefined?object.source.toString():"";return value?Qt.resolvedUrl(value).toString():"";}
    function held(target){return target.object && target.object.source!==undefined && (source(target.object)===target.uri || (target.media && target.object.playbackState!==MediaPlayer.StoppedState));}
    function targets(entry){
        if(!entry.owner)return entry.targets;
        return entry.objects.map(object=>({object:object,uri:source(object),media:entry.media.includes(object)})).filter(target=>!!target.uri);
    }
    function same(left,right){return left.length===right.length && left.every((object,index)=>object===right[index]);}
    function registerConsumer(consumer){
        if(!consumer || consumer.alive!==true)return;
        const existing=consumers.find(entry=>entry.owner===consumer);
        if(existing && same(existing.objects,consumer.sourceObjects) && same(existing.media,consumer.mediaObjects))return;
        unregisterConsumer(consumer);
        consumers=consumers.concat([{owner:consumer,objects:consumer.sourceObjects.slice(),media:consumer.mediaObjects.slice()}]);
    }
    function unregisterConsumer(consumer){
        // Adapter removal or rebinding is not source retirement. Keep exact
        // source-owner references until their old sources clear or they die.
        consumers=consumers.map(entry=>entry.owner===consumer?{owner:null,targets:targets(entry)}:entry);
        prune();
    }
    function prune(){consumers=consumers.filter(entry=>entry.owner || entry.targets.some(root.held));}
    function retire(requestIds,uri){
        if(!requestIds.length)return;
        if(uri)uri=Qt.resolvedUrl(uri).toString();
        let references=[];
        if(uri)for(const entry of consumers)references=references.concat(targets(entry).filter(target=>target.uri===uri));
        pending=pending.concat([{ids:requestIds.slice(),targets:references}]);
        Qt.callLater(root.check);
    }
    function check(){
        const ready=[],waiting=[];
        for(const entry of pending){
            if(entry.targets.some(root.held))waiting.push(entry);else ready.push(...entry.ids);
        }
        pending=waiting;prune();
        if(ready.length)retired([...new Set(ready)]);
    }
    // This observes cooperative source retirement; it does not infer that a
    // decoder's internal worker thread has completed.
    Timer {interval:16;repeat:true;running:root.busy || root.consumers.some(entry=>!entry.owner);onTriggered:root.check()}
}

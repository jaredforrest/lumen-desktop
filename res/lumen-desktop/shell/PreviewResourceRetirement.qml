import QtQuick
Item {
    id:root
    property var consumers:[]
    property var pending:[]
    readonly property bool busy:pending.length>0
    signal retired(var requestIds)
    function registerConsumer(consumer){if(!consumers.includes(consumer))consumers=consumers.concat([consumer]);}
    function unregisterConsumer(consumer){consumers=consumers.filter(item=>item!==consumer);}
    function retire(requestIds,uri){
        if(!requestIds.length)return;
        if(uri)uri=Qt.resolvedUrl(uri).toString();
        const targets=uri?consumers.filter(item=>item && item.alive===true && item.sources.includes(uri)).map(item=>({consumer:item,media:item.mediaSources.includes(uri)})):[];
        pending=pending.concat([{ids:requestIds.slice(),uri:uri || "",targets:targets}]);
        Qt.callLater(root.check);
    }
    function check(){
        const ready=[],waiting=[];
        for(const entry of pending){
            const held=entry.targets.some(target=>target.consumer && target.consumer.alive===true && (target.consumer.sources.includes(entry.uri) || (target.media && !target.consumer.mediaStopped)));
            if(held)waiting.push(entry);else ready.push(...entry.ids);
        }
        pending=waiting;
        if(ready.length)retired([...new Set(ready)]);
    }
    // This observes cooperative source retirement; it does not infer that a
    // decoder's internal worker thread has completed.
    Timer {interval:16;repeat:true;running:root.busy;onTriggered:root.check()}
}

.pragma library

function begin(desktops, id, revision, sessionEpoch) {
    const item=desktops.find(item=>item.logicalId===id);
    if(!item || !item.available || !Number.isInteger(revision) || !sessionEpoch)return null;
    const order=desktops.filter(other=>other.outputId===item.outputId && other.available).map(other=>other.logicalId);
    if(new Set(order).size!==order.length)return null;
    return {id:id,outputId:item.outputId,original:order.slice(),order:order.slice(),revision:revision,sessionEpoch:sessionEpoch};
}
function move(gesture, target) {
    if(!gesture || !Number.isInteger(target) || target<0 || target>=gesture.order.length)return gesture;
    const order=gesture.order.slice(),index=order.indexOf(gesture.id);
    if(index<0)return null;
    order.splice(index,1);order.splice(target,0,gesture.id);
    return Object.assign({},gesture,{order:order});
}
function request(gesture, revision, sessionEpoch) {
    if(!gesture || gesture.revision!==revision || gesture.sessionEpoch!==sessionEpoch)return null;
    if(gesture.order.every((id,index)=>id===gesture.original[index]))return null;
    return {action:"desktop-reorder",outputId:gesture.outputId,order:gesture.order.slice(),revision:gesture.revision,sessionEpoch:gesture.sessionEpoch};
}

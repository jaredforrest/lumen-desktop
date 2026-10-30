.pragma library
// Freeze application membership at invocation; preview never changes focus/MRU.
function snapshot(windows, history, identity) {
    const ordered=history.filter(w=>windows.includes(w)).concat(windows.filter(w=>!history.includes(w)));
    const seen=Object.create(null);
    return ordered.filter(w=>{
        const key=identity(w);
        if (!key || seen[key]) return false;
        seen[key]=true;return true;
    }).map(w=>({key:identity(w),window:w}));
}
function step(index, count, delta) {
    return count ? (index+delta+count)%count : -1;
}
function validSelection(items, index, windows, identity) {
    if (index<0 || index>=items.length) return null;
    // A replacement window of the same application may be activated; a new
    // application can never inherit a vanished application's snapshot slot.
    return windows.some(w=>identity(w)===items[index].key) ? items[index] : null;
}
function creationOrder(previous, windows) {
    return previous.filter(w=>windows.includes(w)).concat(windows.filter(w=>!previous.includes(w)));
}
function cycleWindow(order, windows, active, identity, delta) {
    if (!active) return null;
    const candidates=creationOrder(order,windows).filter(w=>identity(w)===identity(active));
    const current=candidates.indexOf(active);
    return current<0 ? null : candidates[step(current,candidates.length,delta)];
}

// Retained zero-window applications keep their application-level MRU slot.
function applicationOrder(items, history) {
    const result=[];
    for(const key of history || []){const item=items.find(item=>item.key===key);if(item && !result.includes(item))result.push(item);}
    return result.concat(items.filter(item=>!result.includes(item)));
}

// Recovery is ordering preference data, never a captured activation request.
function recoverApplicationOrder(local, recovered, active, previous, current, changedEpoch, liveLocal) {
    const kept=changedEpoch?[]:(local || []).filter(id=>{
        const old=previous[id],now=current[id];
        return id===active || (liveLocal || []).includes(id) || (Array.isArray(old) && Array.isArray(now) && old.some(instance=>now.includes(instance)));
    });
    const result=[];
    for(const id of (active?[active]:[]).concat(kept,recovered || [])){
        if(typeof id==="string" && id && !result.includes(id))result.push(id);
    }
    return result.slice(0,512);
}

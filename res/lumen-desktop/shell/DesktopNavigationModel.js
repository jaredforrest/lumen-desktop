.pragma library

// Operate on the presented layout, never the provider's hidden row order.
function neighbour(items,key,direction) {
    const visible=items.filter(item=>!item.overflow),origin=visible.find(item=>item.key===key);
    if(!origin)return visible.length?visible[0].key:null;
    const horizontal=direction==="left" || direction==="right",sign=direction==="left" || direction==="up"?-1:1;
    const primary=item=>horizontal?item.x+item.width/2:item.y+item.height/2;
    const secondary=item=>horizontal?item.y+item.height/2:item.x+item.width/2;
    const extent=item=>horizontal?item.height:item.width;
    const candidates=visible.filter(item=>item.key!==key && sign*(primary(item)-primary(origin))>0).map(item=>{
        const across=Math.abs(secondary(item)-secondary(origin));
        return {key:item.key,aligned:across<(extent(item)+extent(origin))/2,along:sign*(primary(item)-primary(origin)),across:across};
    });
    candidates.sort((a,b)=>Number(b.aligned)-Number(a.aligned) || a.along-b.along || a.across-b.across || a.key.localeCompare(b.key));
    return candidates.length?candidates[0].key:key;
}

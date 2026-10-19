function position(value) {
    return value && Number.isFinite(value.x) && Number.isFinite(value.y)
        ? {x:Math.max(0,Math.min(1,value.x)),y:Math.max(0,Math.min(1,value.y))}:null;
}
function normalize(items) {
    if(!Array.isArray(items))return null;
    let seen=new Set(),result=[];
    for(const value of items.slice(0,32)) {
        if(!value || typeof value.id!=="string" || !/^[a-z0-9-]{1,80}$/.test(value.id) || seen.has(value.id) || !["calendar","clock"].includes(value.type))continue;
        seen.add(value.id);
        let positions=Object.create(null);
        if(value.positions && typeof value.positions==="object" && !Array.isArray(value.positions))for(const output of Object.keys(value.positions).slice(0,32)) {
            const p=position(value.positions[output]);
            if(output.length>0 && output.length<=256 && p)positions[output]=p;
        }
        result.push({id:value.id,type:value.type,size:["small","medium","large"].includes(value.size)?value.size:"medium",
            output:typeof value.output==="string" && value.output.length>0 && value.output.length<=256?value.output:"*",
            style:value.style==="agenda"?"agenda":"month",hourCycle:["locale","12","24"].includes(value.hourCycle)?value.hourCycle:"locale",
            positions:positions});
    }
    return result;
}
function effective(preferences) {
    if(Array.isArray(preferences.desktopItems))return normalize(preferences.desktopItems);
    return [{id:"legacy-calendar",type:"calendar",size:"medium",output:"*",style:preferences.desktopStyle || "month",hourCycle:"locale",positions:preferences.positions || {}}];
}
function updated(items,id,patch) {
    if(!items.some(item=>item.id===id))return null;
    return normalize(items.map(item=>item.id===id?Object.assign({},item,patch,{id:item.id,type:item.type}):item));
}
function dimensions(item,width,height,barHeight) {
    const wantedWidth=item.size==="small"?214:344;
    const wantedHeight=item.type==="clock"?(item.size==="small"?142:item.size==="large"?248:182):(item.size==="small"?206:item.size==="large"?530:388);
    return {width:Math.max(1,Math.min(wantedWidth,width-24)),height:Math.max(1,Math.min(wantedHeight,height-barHeight-100))};
}
function placed(item,output,index,width,height,barHeight,positionOverride) {
    const size=dimensions(item,width,height,barHeight);
    const saved=position(positionOverride) || position(item.positions[output]) || {x:Math.min(.9,.02+(index%3)*.3),y:Math.min(.9,.03+Math.floor(index/3)*.28)};
    const left=Math.min(8,Math.max(0,width-size.width)),top=Math.min(barHeight+10,Math.max(0,height-size.height)),right=Math.max(left,width-size.width-8),bottom=Math.max(top,height-size.height-80);
    return {x:Math.max(left,Math.min(right,Math.round(saved.x*Math.max(0,width-size.width)))),y:Math.max(top,Math.min(bottom,Math.round(saved.y*Math.max(0,height-size.height-80)))),width:size.width,height:size.height};
}

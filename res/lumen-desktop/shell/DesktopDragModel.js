.pragma library

// A gesture uses one frozen snapshot and one shared clamped delta. Clamping each
// icon independently would distort the arrangement at a screen edge.
function begin(items,key,selectedPaths,grouping) {
    const target=items.find(item=>item.key===key);
    if(!target || target.overflow)return [];
    if(target.stack)return items.filter(item=>item.key===key || item.parentStack===key && !item.overflow).map(item=>({key:item.key,x:item.x,y:item.y,width:item.width,height:item.height,stack:item.stack}));
    if(grouping!=="none")return [];
    const selected=selectedPaths.includes(target.row.path)?selectedPaths:[target.row.path];
    return items.filter(item=>!item.stack && !item.overflow && selected.includes(item.row.path)).map(item=>({key:item.key,x:item.x,y:item.y,width:item.width,height:item.height,stack:false}));
}
function translate(snapshot,dx,dy,width,height) {
    if(!snapshot.length || !Number.isFinite(dx) || !Number.isFinite(dy))return [];
    const minX=Math.min(...snapshot.map(item=>item.x)),minY=Math.min(...snapshot.map(item=>item.y));
    const maxX=Math.max(...snapshot.map(item=>item.x+item.width)),maxY=Math.max(...snapshot.map(item=>item.y+item.height));
    dx=Math.max(-minX,Math.min(Math.max(0,width-maxX),dx));dy=Math.max(-minY,Math.min(Math.max(0,height-maxY),dy));
    return snapshot.map(item=>Object.assign({},item,{x:item.x+dx,y:item.y+dy}));
}
function normalized(items,width,height){
    const result={};for(const item of items)result[item.key]=[Math.max(0,Math.min(1,item.x/Math.max(1,width-item.width))),Math.max(0,Math.min(1,item.y/Math.max(1,height-item.height)))];return result;
}

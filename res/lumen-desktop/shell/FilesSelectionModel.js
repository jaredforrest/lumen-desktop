.import "FileIdentity.js" as Identity
// Exact selection references; focus and range anchor are independent of membership.
function ref(row){
    if(!row || typeof row.path!=="string" || !row.path.startsWith("/") || row.path.length>4096)return null;
    const identity=Identity.normalize(row.identity);
    return identity?{path:row.path,identity:identity}:null;
}
function key(value){const item=ref(value);return item?JSON.stringify([item.path,item.identity.device,item.identity.inode]):"";}
function locate(rows,value){
    const item=ref(value);if(!item)return -1;
    const exact=rows.findIndex(row=>row.path===item.path && Identity.equal(row.identity,item.identity));
    if(exact>=0)return exact;
    const matches=rows.map((row,index)=>({row:row,index:index})).filter(entry=>Identity.equal(entry.row.identity,item.identity));
    return matches.length===1?matches[0].index:-1;
}
function capture(rows,paths,current,anchor){
    if(!Array.isArray(rows) || rows.length>5000)return null;
    return {selection:rows.filter(row=>paths.includes(row.path)).map(ref).filter(Boolean),focus:ref(rows[current]),anchor:ref(rows[anchor])};
}
function reconcile(rows,snapshot){
    snapshot=snapshot || {selection:[],focus:null,anchor:null};
    const indexes=[...new Set((snapshot.selection || []).map(value=>locate(rows,value)).filter(index=>index>=0))];
    return {paths:indexes.map(index=>rows[index].path),current:locate(rows,snapshot.focus),anchor:locate(rows,snapshot.anchor)};
}
function revision(rows){return JSON.stringify(rows.map(row=>{const value=ref(row);return value?[value.path,value.identity.device,value.identity.inode]:null;}));}
function rectangle(start,end){
    if(![start.x,start.y,end.x,end.y].every(value=>Number.isFinite(value) && Math.abs(value)<=10000000))return null;
    return {x:Math.min(start.x,end.x),y:Math.min(start.y,end.y),width:Math.abs(end.x-start.x),height:Math.abs(end.y-start.y)};
}
function intersects(rect,box){
    return rect && box && [box.x,box.y,box.width,box.height].every(Number.isFinite) && box.width>0 && box.height>0 && rect.width>0 && rect.height>0 && rect.x<box.x+box.width && rect.x+rect.width>box.x && rect.y<box.y+box.height && rect.y+rect.height>box.y;
}
function hits(rect,boxes){return boxes.filter(box=>intersects(rect,box)).map(box=>ref(box.row)).filter(Boolean);}
function combine(snapshot,hitRows,mode){
    if(!snapshot || !["replace","toggle","add"].includes(mode))return null;
    const previous=(snapshot.selection || []).map(ref).filter(Boolean),hits=[...new Map(hitRows.map(ref).filter(Boolean).map(item=>[key(item),item])).values()],values=new Map();
    if(mode!=="replace")for(const item of previous)values.set(key(item),item);
    for(const item of hits){const id=key(item);if(mode==="toggle" && values.has(id))values.delete(id);else values.set(id,item);}
    return {selection:[...values.values()],focus:ref(snapshot.focus),anchor:ref(snapshot.anchor)};
}
function begin(rows,paths,current,anchor,context,mode){
    const snapshot=capture(rows,paths,current,anchor);
    if(!snapshot || rows.some(row=>!ref(row)) || typeof context!=="string" || !["replace","toggle","add"].includes(mode))return null;
    return {revision:revision(rows),context:context,snapshot:snapshot,mode:mode};
}
function finish(transaction,rows,context,hitRows){
    if(!transaction || transaction.context!==context || transaction.revision!==revision(rows))return null;
    const preview=combine(transaction.snapshot,hitRows,transaction.mode);
    return preview?reconcile(rows,preview):null;
}
if(typeof module!=="undefined")module.exports={ref,key,locate,capture,reconcile,revision,rectangle,intersects,hits,combine,begin,finish};

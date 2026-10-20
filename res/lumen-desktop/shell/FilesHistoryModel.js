.import "FileIdentity.js" as Identity
// Runtime-only presentation snapshots. No directory preferences are persisted.
function path(entry){return typeof entry==="string"?entry:entry && entry.path || "";}
function ref(row){if(!row || typeof row.path!=="string" || row.path.length>4096)return null;return {path:row.path,identity:row.identity!==undefined && row.identity!==null?(Identity.normalize(row.identity) || {invalid:true}):null};}
function locate(rows,item){
    if(!item)return -1;
    const known=Identity.normalize(item.identity);
    if(item.identity!==undefined && item.identity!==null && !known)return -1;
    const exact=rows.findIndex(row=>row.path===item.path && (!known?(row.identity===undefined || row.identity===null):Identity.equal(row.identity,known)));
    if(exact>=0 || !known)return exact;
    const candidates=rows.map((row,index)=>({row:row,index:index})).filter(value=>Identity.equal(value.row.identity,known));
    return candidates.length===1?candidates[0].index:-1;
}
function offset(value){return Number.isFinite(value)?Math.max(0,Math.min(10000000,value)):0;}
function clone(value){return value===undefined?null:JSON.parse(JSON.stringify(value));}
function presentation(value){
    value=value || {};const candidates=Array.isArray(value.selection)?value.selection:[];const overflow=value.selectionOverflow===true || candidates.length>512 || candidates.reduce((size,item)=>size+(item && typeof item.path==="string"?item.path.length:0),0)>65536;const result={view:["grid","list","columns","gallery"].includes(value.view)?value.view:"grid",query:typeof value.query==="string"?value.query.slice(0,4096):"",selectionOverflow:overflow,selection:overflow?[]:(Array.isArray(value.selection)?value.selection:[]).map(ref).filter(Boolean),current:overflow?null:ref(value.current),anchor:overflow?null:ref(value.anchor),scroll:{}};
    for(const name of ["grid","list","gallery"]){const s=(value.scroll || {})[name] || {};result.scroll[name]={offset:offset(s.offset),delta:offset(s.delta),anchor:ref(s.anchor)};}
    const c=(value.scroll || {}).columns || {};result.scroll.columns={x:offset(c.x),columns:(Array.isArray(c.columns)?c.columns:[]).slice(0,3).map(s=>({path:typeof s.path==="string" && s.path.length<=4096?s.path:"",offset:offset(s.offset),anchor:ref(s.anchor),delta:offset(s.delta)}))};
    return result;
}
function save(history,index,state){if(index<0 || index>=history.length)return history;const next=history.slice(),old=next[index];next[index]={id:old.id || "legacy-"+index,path:path(old),presentation:presentation(state)};return next;}
function commit(history,index,target,requested,id){
    if(requested>=0 && requested<history.length && path(history[requested])===target)return {history:history,index:requested,restore:true};
    if(path(history[index])===target)return {history:history,index:index,restore:false};
    const next=history.slice(0,index+1).concat([{id:id,path:target,presentation:null}]).slice(-128);return {history:next,index:next.length-1,restore:false};
}
function reconcile(state,rows){state=presentation(state);const selection=state.selection.map(item=>locate(rows,item)).filter(i=>i>=0),current=locate(rows,state.current),anchor=locate(rows,state.anchor);return {paths:selection.map(i=>rows[i].path),selected:current,anchor:anchor>=0?anchor:-1};}
if(typeof module!=="undefined")module.exports={path,ref,locate,offset,clone,presentation,save,commit,reconcile};

.pragma library

function identities(rows) {
    const counts={},result=[];
    for(const row of rows){const value=row.identity || {},base=String(value.device)+":"+String(value.inode);counts[base]=(counts[base] || 0)+1;result.push(base);}
    // Hard links share an inode but remain separate desktop entries. Their path
    // suffix is deterministic; ordinary entries keep their identity on rename.
    return result.map((base,index)=>counts[base]>1?base+"-"+pathHash(rows[index].name):base);
}
function pathHash(value){let a=2166136261,b=2246822507;for(let i=0;i<value.length;i++){a=Math.imul(a^value.charCodeAt(i),16777619);b=Math.imul(b^value.charCodeAt(i),3266489909);}return (a>>>0).toString(16).padStart(8,"0")+(b>>>0).toString(16).padStart(8,"0");}
function category(row,mode,now) {
    if(mode==="kind")return row.directory?"Folders":String(row.kind).startsWith("image/")?"Images":String(row.kind).startsWith("audio/")?"Audio":String(row.kind).startsWith("video/")?"Video":"Documents";
    if(mode==="tags")return row.tags && row.tags.length?row.tags.map(tag=>tag.id).slice().sort().join(", "):"Untagged";
    if(mode==="modified"){
        const today=new Date(now);today.setHours(0,0,0,0);const date=new Date(row.mtime*1000);date.setHours(0,0,0,0);const days=Math.round((today-date)/86400000);
        return days<=0?"Today":days===1?"Yesterday":days<7?"Previous 7 Days":days<30?"Previous 30 Days":"Older";
    }
    return "";
}
function bounded(value){return Math.max(0,Math.min(1,Number.isFinite(value)?value:0));}
function point(x,y,width,height,itemWidth,itemHeight){return [bounded(x/Math.max(1,width-itemWidth)),bounded(y/Math.max(1,height-itemHeight))];}
function build(rows,layout,expanded,width,height,now) {
    const ids=identities(rows),groups={},items=[],mode=layout.grouping || "none",positions=layout.positions || {};
    rows.forEach((row,index)=>{
        const item={key:ids[index],row:row,sourceIndex:index,stack:false};
        if(mode==="none" || row.directory)items.push(item);
        else {const label=category(row,mode,now);if(!groups[label]){groups[label]=[];items.push({key:"stack:"+mode+":"+label,label:label,stack:true,children:groups[label]});}groups[label].push(item);}
    });
    const flattened=[];
    for(const item of items){flattened.push(item);if(item.stack && expanded.includes(item.key))for(const child of item.children)flattened.push(child);}
    const cellWidth=108,cellHeight=100,columns=Math.max(1,Math.floor(width/cellWidth)),slots=Math.max(1,Math.floor(height/cellHeight)),occupied=new Set();
    function reserve(x,y){const column=Math.max(0,Math.min(columns-1,Math.floor(x/cellWidth))),row=Math.max(0,Math.min(slots-1,Math.floor(y/cellHeight)));occupied.add(column+":"+row);}
    // Saved placements claim their slots before automatic items are arranged.
    if(mode==="none")for(const item of flattened)if(positions[item.key])reserve(bounded(positions[item.key][0])*Math.max(0,width-cellWidth),bounded(positions[item.key][1])*Math.max(0,height-cellHeight));
    let automatic=0;
    return flattened.map(item=>{
        let x,y;const saved=mode==="none"?positions[item.key]:null;
        if(saved){x=bounded(saved[0])*Math.max(0,width-cellWidth);y=bounded(saved[1])*Math.max(0,height-cellHeight);}
        else {
            while(occupied.has((columns-1-Math.floor(automatic/slots))+":"+(automatic%slots)) && automatic<columns*slots)automatic++;
            const column=columns-1-Math.floor(automatic/slots),row=automatic%slots;
            x=Math.max(0,column*cellWidth);y=row*cellHeight;reserve(x,y);automatic++;
        }
        return Object.assign({},item,{x:x,y:y,width:cellWidth,height:cellHeight,overflow:automatic>columns*slots});
    });
}

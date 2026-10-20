.pragma library
function facts(value){return Array.isArray(value) && value.length===6 && value.every(v=>typeof v==="string")?value.slice():null;}
function capture(rows,row,generation,folder){
    const version=row?facts(row.actionFacts):null;
    if(!row || !rows.includes(row) || typeof row.path!=="string" || !version)return null;
    return {row:row,path:row.path,facts:version,generation:generation,folder:folder,footer:false};
}
function captureFolder(folder,version,generation){const selected=facts(version);return selected?{row:null,path:folder,facts:selected,generation:generation,folder:folder,footer:true}:null;}
function valid(intent,rows,generation,folder,folderFacts){
    if(!intent || intent.generation!==generation || intent.folder!==folder)return false;
    if(intent.footer)return intent.path===folder && JSON.stringify(intent.facts)===JSON.stringify(folderFacts);
    return rows.includes(intent.row) && intent.path===intent.row.path && JSON.stringify(intent.facts)===JSON.stringify(intent.row.actionFacts);
}

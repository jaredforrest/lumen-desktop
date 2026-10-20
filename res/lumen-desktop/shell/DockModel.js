.pragma library
// Keep app identity and order stable as windows open and close.
function canonical(id) { return id ? id.replace(/\.desktop$/, "") : ""; }
function sameEntry(a,b) { return canonical(a)===canonical(b); }
function build(reference, entries, running, pins, recent, showRecent, folders, files, minimized) {
    const aliases = {writer:"ai.storyteller.wordcraft",calc:"ai.storyteller.gridcraft",impress:"ai.storyteller.deckcraft",obsidian:"obsidian",contacts:"org.gnome.Contacts"};
    const lookup = id => entries.find(e => sameEntry(e.id,id));
    const result = reference.map(item => {
        const owned = {files:"lumen-files",settings:"lumen-system-settings",calendar:"lumen-calendar-app"};
        const id = owned[item.action] || aliases[item.appId] || item.appId;
        return Object.assign({}, item, {entry:id ? lookup(id) : null});
    }).filter(item => item.entry || item.action);
    const fixed = item => ["files","applications","downloads","trash"].includes(item.key);
    const tail = result.filter(item => ["downloads","trash"].includes(item.key));
    let apps = result.filter(item => !["downloads","trash"].includes(item.key));
    if (Array.isArray(pins)) {
        const fixedApps=apps.filter(fixed);
        apps=fixedApps.concat(pins.map(id=>{
            const entry=lookup(id);
            if(!entry || fixedApps.some(item=>item.entry && sameEntry(item.entry.id,entry.id)))return null;
            return apps.find(item=>item.entry && sameEntry(item.entry.id,entry.id)) ||
                {key:entry.id,name:entry.name,entry:entry,canvas:68,bottom:13,image:"",action:null};
        }).filter(Boolean));
    }
    const append=(entry,section)=>{
        if(!entry || apps.some(item=>item.entry && sameEntry(item.entry.id,entry.id)))return;
        apps.push({key:entry.id,name:entry.name,entry:entry,section:section,canvas:68,bottom:13,image:"",action:null});
    };
    for (const entry of running) append(entry,"running");
    if(showRecent)for(const entry of (recent || []).filter(entry=>!apps.some(item=>item.entry && sameEntry(item.entry.id,entry.id))).slice(0,3))append(entry,"recent");
    const stacks=(folders || []).map(folder=>({key:"folder:"+folder.path,name:folder.path.split("/").filter(Boolean).pop() || "/",action:"folder",section:"documents",folder:folder,canvas:68,bottom:13,image:""}));
    const fileAliases=(files || []).map(path=>({key:"file:"+path,name:path.split("/").pop(),action:"file",section:"documents",path:path,canvas:68,bottom:13,image:""}));
    return apps.concat(minimized || []).concat(stacks).concat(fileAliases).concat(tail.map(item=>Object.assign({},item,{section:"documents"}))).map((item,index,array) => Object.assign({},item,{center:(index+0.5)*2142/array.length,separatorBefore:index>0 && ["recent","documents"].includes(item.section) && array[index-1].section!==item.section}));
}
function configuredPins(reference, entries, pins) {
    if(Array.isArray(pins))return pins.slice();
    return build(reference,entries,[]).filter(item=>item.entry && !["files","applications","downloads","trash"].includes(item.key)).map(item=>item.entry.id);
}
function updatePins(pins,id,action) {
    const next=pins.filter(pin=>!sameEntry(pin,id));
    const index=pins.findIndex(pin=>sameEntry(pin,id));
    if(action==="remove")return next;
    if(index<0){next.push(id);return next;}
    const destination=action==="earlier"?Math.max(0,index-1):action==="later"?Math.min(next.length,index+1):index;
    next.splice(destination,0,id);return next;
}
function recentApplications(previous, entry, limit) {
    return entry?[entry].concat(previous.filter(item=>!sameEntry(item.id,entry.id))).slice(0,limit || 12):previous;
}

function movePin(pins,source,target) {
    if(sameEntry(source,target) || !pins.some(id=>sameEntry(id,target)))return pins.slice();
    const next=pins.filter(id=>!sameEntry(id,source));
    const index=next.findIndex(id=>sameEntry(id,target));
    next.splice(index,0,source);return next;
}

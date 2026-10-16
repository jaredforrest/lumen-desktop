.pragma library
// Keep app identity and order stable as windows open and close.
function build(reference, entries, running) {
    const aliases = {writer:"ai.storyteller.wordcraft",calc:"ai.storyteller.gridcraft",impress:"ai.storyteller.deckcraft",obsidian:"obsidian",contacts:"org.gnome.Contacts"};
    const lookup = id => entries.find(e => e.id === id || e.id === id + ".desktop");
    const result = reference.map(item => {
        const owned = {files:"lumen-files",settings:"lumen-system-settings",calendar:"lumen-calendar-app"};
        const id = owned[item.action] || aliases[item.appId] || item.appId;
        return Object.assign({}, item, {entry:id ? lookup(id) : null});
    }).filter(item => item.entry || item.action);
    const tail = result.filter(item => ["downloads","trash"].includes(item.key));
    const apps = result.filter(item => !["downloads","trash"].includes(item.key));
    for (const entry of running) {
        if (!apps.some(item => item.entry && item.entry.id === entry.id))
            apps.push({key:entry.id,name:entry.name,entry:entry,canvas:68,bottom:13,image:"",action:null});
    }
    return apps.concat(tail).map((item,index,array) => Object.assign({},item,{center:(index+0.5)*2142/array.length}));
}

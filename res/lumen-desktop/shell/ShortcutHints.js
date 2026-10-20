// Presentation only. Dispatch and collision authority remain in the registry.
function read(value) {
    if(!value || value.version!==1 || value.valid!==true || !Array.isArray(value.bindings) || value.bindings.length>4096 || !Array.isArray(value.hints) || value.hints.length>4096)throw Error("Invalid shortcut registry");
    for(const group of value.hints){
        if(!group || typeof group.action!=="string" || !["global","application","current-application"].includes(group.scope) || typeof group.appId!=="string" || !Array.isArray(group.bindings) || !group.bindings.length)throw Error("Invalid shortcut hints");
        for(const item of group.bindings){
            const physical=item.physical;
            if(!physical || !Array.isArray(physical.modifiers) || physical.modifiers.length>8 || physical.modifiers.some(key=>typeof key!=="string" || !["Shift","Lock","Control","Mod1","Mod2","Mod3","Mod4","Mod5"].includes(key)) || typeof physical.key!=="string" || !physical.key || physical.key.length>512 || typeof item.chord!=="string" || item.chord.length>512)throw Error("Invalid shortcut display");
            const source=value.bindings.find(row=>row.id===item.id);
            if(!source || source.enabled!==true || source.action!==group.action || source.scope!==group.scope || source.appId!==group.appId || source.mode!=="default" || source.owner==="reserved" || source.action==="consumer.binding" || source.chord!==item.chord || JSON.stringify(source.physical)!==JSON.stringify(item.physical) || item.physical.trigger!=="press" || item.physical.device!=="*")throw Error("Shortcut hint is not an effective binding");
        }
    }
    return {bindings:value.bindings,hints:value.hints};
}
function display(physical) {
    const modifiers={Shift:"Shift",Lock:"Caps Lock",Control:"Ctrl",Mod1:"Alt",Mod2:"Mod2",Mod3:"Mod3",Mod4:"Super",Mod5:"Mod5"};
    const keys={left:"Left",right:"Right",up:"Up",down:"Down",grave:"`",return:"Enter",escape:"Esc",space:"Space",tab:"Tab"};
    return physical.modifiers.map(key=>modifiers[key] || key).concat([keys[physical.key] || (physical.key.length===1?physical.key.toUpperCase():physical.key.replace(/^f(\d+)$/, "F$1"))]).join("+");
}
function hint(groups,action,appId) {
    const local=appId?groups.find(group=>group.action===action && group.scope==="application" && group.appId===appId):null;
    const group=local || groups.find(group=>group.action===action && group.scope==="global");
    return group?group.bindings.map(item=>display(item.physical)).join(" / "):"";
}
function menuAction(action) {
    return ({fullscreen:"window.fullscreen.toggle",fill:"window.fill","show-desktop":"shell.show_desktop",mute:"audio.key.mute",up:"audio.key.up",down:"audio.key.down","status:Controls":"shell.control_centre.toggle"})[action] || "";
}

// Presentation only. Dispatch and collision authority remain in the registry.
function actionName(action) {
    const names={
        "shell.app_switcher":"Switch applications","shell.app_switcher.reverse":"Switch applications backwards","shell.app_switcher.commit":"Confirm application selection",
        "shell.app_windows":"Show application windows","shell.control_centre.toggle":"Show controls","shell.dock_focus":"Focus the Dock",
        "shell.notification_centre.toggle":"Show notifications","shell.overview":"Show all windows","shell.show_desktop":"Show desktop",
        "shell.text_tools":"Open text tools","shell.window_groups":"Show window groups","shell.window_next":"Next application window","shell.window_previous":"Previous application window",
        "spaces.switch-next":"Next workspace","spaces.switch-previous":"Previous workspace","window.fill":"Fill the work area",
        "window.fullscreen.toggle":"Toggle full screen","window.layout_menu":"Show window layouts",
        "audio.key.down":"Lower volume","audio.key.up":"Raise volume","audio.key.mute":"Mute or unmute",
        "media.key.next":"Next track","media.key.previous":"Previous track","media.key.play-pause":"Play or pause"
    };
    const slot=/^application\.command_slot\.([1-9]|1[0-2])$/.exec(action);
    return names[action] || (slot?"Application command "+slot[1]:"Custom shortcut");
}
function readableChord(chord) {
    const names={Mod4:"Super",Mod1:"Alt",Control:"Ctrl"};
    return String(chord).split("+").map(key=>names[key] || (key.length===1?key.toUpperCase():key)).join("+");
}
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
    return group?display(group.bindings[0].physical):"";
}
function menuAction(action) {
    return ({fullscreen:"window.fullscreen.toggle",fill:"window.fill","show-desktop":"shell.show_desktop",mute:"audio.key.mute",up:"audio.key.up",down:"audio.key.down","status:Controls":"shell.control_centre.toggle"})[action] || "";
}

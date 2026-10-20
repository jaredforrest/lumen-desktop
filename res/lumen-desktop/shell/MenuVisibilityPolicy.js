.pragma library
function normalize(values) {
    const result=Object.assign({},values);
    if(Object.prototype.hasOwnProperty.call(result,"fullscreenVisibility")) {
        if(Object.prototype.hasOwnProperty.call(result,"visibility"))throw Error("Conflicting menu visibility settings");
        if(!["automatic","always"].includes(result.fullscreenVisibility))throw Error("Invalid legacy menu visibility");
        result.visibility=result.fullscreenVisibility==="always"?"always":"desktop";
        delete result.fullscreenVisibility;
    }
    return result;
}
function automatic(values,fullscreen) {
    const mode=normalize(values).visibility || "desktop";
    return mode==="auto_hide" || (fullscreen?mode==="desktop":mode==="fullscreen");
}

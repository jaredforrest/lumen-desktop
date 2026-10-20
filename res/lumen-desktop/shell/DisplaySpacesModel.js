// Routing is a usability fallback, separate from authoritative applied policy.
function text(value){return typeof value==="string" && value.length>0 && value.length<=512 && !/[\x00-\x1f\x7f]/.test(value);}
function generation(value){return typeof value==="string" && /^(0|[1-9][0-9]*)$/.test(value) && (value.length<20 || (value.length===20 && value<="18446744073709551615"));}
function identity(value){
    if(value===null)return true;
    if(!value || typeof value!=="object" || Array.isArray(value) || Object.keys(value).sort().join(",")!=="kind,make,model,name,serial")return false;
    if(value.kind==="connector")return text(value.name) && value.make===null && value.model===null && value.serial===null;
    return value.kind==="display" && value.name===null && text(value.make) && text(value.model) && text(value.serial) && !["Unknown","0"].includes(value.serial);
}
function identityKey(value){return value.kind==="connector"?JSON.stringify(["connector",value.name]):JSON.stringify(["display",value.make,value.model,value.serial]);}
function valid(value,previous){
    if(!value || typeof value!=="object" || value.protocolVersion!==1)return false;
    for(const key of ["available","applied","separateDisplays","independent","nativeSupported"])if(typeof value[key]!=="boolean")return false;
    if(typeof value.error!=="string" || value.error.length>2048 || typeof value.primary!=="string" || !identity(value.primaryDisplay))return false;
    if(!Array.isArray(value.outputs) || value.outputs.length>64)return false;
    const names=new Set(),identities=new Set();
    for(const row of value.outputs){
        if(!row || !text(row.name) || !text(row.label) || !row.identity || !identity(row.identity))return false;
        const key=identityKey(row.identity);
        if(names.has(row.name) || identities.has(key))return false;
        names.add(row.name);identities.add(key);
    }
    if(value.available && (!names.has(value.primary) || !text(value.primary)))return false;
    if(!value.available && value.primary!=="")return false;
    if(value.applied && !value.available)return false;
    if(value.applied && value.primaryDisplay){
        const preferred=value.outputs.find(row=>identityKey(row.identity)===identityKey(value.primaryDisplay) || (value.primaryDisplay.kind==="connector" && row.name===value.primaryDisplay.name));
        if(preferred && preferred.name!==value.primary)return false;
    }
    if(value.nativeSupported){
        if((value.independent && !value.separateDisplays) || (value.applied && value.separateDisplays && !value.independent))return false;
        if(typeof value.session!=="string" || !/^[a-f0-9]{32}$/.test(value.session) || !generation(value.generation))return false;
        if(previous && previous.nativeSupported && value.session===previous.session && generation(previous.generation) && (value.generation.length<previous.generation.length || (value.generation.length===previous.generation.length && value.generation<previous.generation)))return false;
    } else if(value.session!==null || value.generation!==null || value.available || value.applied || !value.independent)return false;
    return true;
}
function routing(previous,value){
    if(value.independent)return {mode:"independent",primary:""};
    if(value.applied && !value.separateDisplays)return {mode:"main",primary:value.primary};
    return previous || {mode:"independent",primary:""};
}
function primary(route,screens){
    const connected=(screens || []).filter(screen=>screen && text(screen.name));
    if(connected.some(screen=>screen.name===route.primary))return route.primary;
    connected.sort((a,b)=>(Number(a.x)||0)-(Number(b.x)||0) || (Number(a.y)||0)-(Number(b.y)||0) || (a.name<b.name?-1:a.name>b.name?1:0));
    return connected.length?connected[0].name:"";
}
function visible(route,output,screens){return route.mode==="independent" || output===primary(route,screens);}
function destination(route,requested,screens){return route.mode==="main"?primary(route,screens):requested;}

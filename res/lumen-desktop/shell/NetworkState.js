.pragma library
function internet(value) {
    const record=item=>item && typeof item==="object" && !Array.isArray(item)?item:{};
    const text=(item,fallback)=>typeof item==="string" && item.length<=512?item:fallback;
    const source=record(value),routes=record(source.routes),vpn=record(source.vpn);
    const route=item=>{const row=record(item);return {available:row.available===true,interface:text(row.interface,"")};};
    return {
        status:["online","offline","degraded","portal","unknown"].includes(source.status)?source.status:"unknown",
        reason:text(source.reason,"unavailable"),message:text(source.message,"Internet monitoring is unavailable."),
        routes:{"4":route(routes["4"]),"6":route(routes["6"])},
        dns:{status:text(record(source.dns).status,"not checked")},
        vpn:{state:text(vpn.state,"unavailable"),selected_route:vpn.selected_route===true,locked_down:vpn.locked_down===true},
        checks:Array.isArray(source.checks)?source.checks.slice(0,32).filter(item=>typeof record(item).ok==="boolean").map(item=>({ok:item.ok})):[]
    };
}
function state(wifi) {
    const value=wifi || {}, phase=value.phase || (value.available?(value.state==='COMPLETED'?'connected':value.state==='DISCONNECTED'?'disconnected':'unknown'):'service-unavailable');
    const labels={disabled:'Wi-Fi Off',disconnected:'Disconnected',associating:'Connecting…',acquiring:'Acquiring address…',disconnecting:'Disconnecting…',unavailable:'Wi-Fi unavailable','service-unavailable':'Wi-Fi service unavailable',unknown:'Status unknown'};
    let label=labels[phase] || 'Status unknown';
    if(phase==='connected')label=({full:'Connected',portal:'Sign-in required',limited:'Limited Internet',none:'No Internet',unknown:'Internet status unknown'})[value.connectivity] || 'Internet status unknown';
    return {label:label,detail:phase==='connected' && value.name?value.name+' · '+label:label,active:phase==='connected',unavailable:phase==='unavailable' || phase==='service-unavailable',phase:phase};
}

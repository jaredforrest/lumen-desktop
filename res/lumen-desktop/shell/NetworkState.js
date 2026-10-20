.pragma library
function state(wifi) {
    const value=wifi || {}, phase=value.phase || (value.available?(value.state==='COMPLETED'?'connected':value.state==='DISCONNECTED'?'disconnected':'unknown'):'service-unavailable');
    const labels={disabled:'Wi-Fi Off',disconnected:'Disconnected',associating:'Connecting…',acquiring:'Acquiring address…',disconnecting:'Disconnecting…',unavailable:'Wi-Fi unavailable','service-unavailable':'Wi-Fi service unavailable',unknown:'Status unknown'};
    let label=labels[phase] || 'Status unknown';
    if(phase==='connected')label=({full:'Connected',portal:'Sign-in required',limited:'Limited Internet',none:'No Internet',unknown:'Internet status unknown'})[value.connectivity] || 'Internet status unknown';
    return {label:label,detail:phase==='connected' && value.name?value.name+' · '+label:label,active:phase==='connected',unavailable:phase==='unavailable' || phase==='service-unavailable',phase:phase};
}

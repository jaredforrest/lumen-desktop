.pragma library

function paths(urls){
    if(!Array.isArray(urls) || !urls.length || urls.length>1000)throw Error("Drop between 1 and 1000 local files.");
    const result=[];
    for(const value of urls){
        const uri=String(value);if(!uri.startsWith("file:///") || uri.includes("?") || uri.includes("#"))throw Error("Only local file drops are supported.");
        let path;try{path=decodeURIComponent(uri.slice(7));}catch(error){throw Error("The dropped file address is invalid.");}
        if(path.length>4096 || /[\u0000-\u001f\u007f]/.test(path))throw Error("The dropped file path is unsupported.");
        if(!result.includes(path))result.push(path);
    }
    return result;
}
function uris(items){
    if(!items.length || items.length>1000)throw Error("Drag between 1 and 1000 local files.");
    const values=items.map(item=>{const path=item.row.path;if(typeof path!=="string" || !path.startsWith("/") || path.length>4096 || /[\u0000-\u001f\u007f]/.test(path))throw Error("The selected file path cannot be offered.");return "file://"+path.split("/").map(encodeURIComponent).join("/");});
    return values.join("\r\n")+"\r\n";
}

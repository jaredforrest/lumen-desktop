.pragma library
.import "FileIdentity.js" as FileIdentity

// Capture presentation identity only. The provider validates actual files and consent.
function capture(context, paths, rows) {
    if (!context || context.path === "trash:" || !Array.isArray(paths) || !paths.length || paths.length > 128)
        return null;
    const selected = [];
    for (const path of paths) {
        if (typeof path !== "string" || !path.startsWith("/") || path.includes("\0") || path.length > 4096 || selected.some(item => item.path === path))
            return null;
        const matches = rows.filter(row => row.path === path);
        if (matches.length !== 1 || matches[0].directory || !matches[0].identity)
            return null;
        const identity = FileIdentity.normalize(matches[0].identity);
        if (!identity)
            return null;
        selected.push({path:path, device:identity.device, inode:identity.inode});
    }
    return {path:context.path, tab:context.tab, selection:selected};
}

function current(intent, context, paths, rows) {
    const live = capture(context, paths, rows);
    return !!intent && !!live && JSON.stringify(intent) === JSON.stringify(live);
}

function requestFor(intent, destination) {
    if (!intent || !Array.isArray(intent.selection) || !intent.selection.length || !destination || destination.state !== "ready" || typeof destination.id !== "string" || !/^[a-z][a-z0-9-]{0,47}$/.test(destination.id) || typeof destination.fingerprint !== "string" || !/^[a-f0-9]{64}$/.test(destination.fingerprint))
        return null;
    const request = {id:destination.id, fingerprint:destination.fingerprint, files:intent.selection.map(row=>Object.assign({},row))};
    if (JSON.stringify(request).length > 16000)
        return null;
    return request;
}

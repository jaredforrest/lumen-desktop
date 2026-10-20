.pragma library
.import "FileIdentity.js" as FileIdentity

// Capture presentation identity only. The provider validates actual files and consent.
function capture(context, paths, rows) {
    if (!context || context.path === "trash:" || !Array.isArray(paths) || !paths.length || paths.length > 128)
        return null;
    const selected = [];
    for (const path of paths) {
        if (typeof path !== "string" || !path.startsWith("/") || selected.some(item => item.path === path))
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

function argumentsFor(intent) {
    return ["lumen-nearby-sharing", "send"].concat(intent.selection.map(item => item.path));
}

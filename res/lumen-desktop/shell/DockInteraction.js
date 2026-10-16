.pragma library
// Press ownership is an item identity, independent of delegate reuse or order.
function releaseTarget(items, pressedKey, index) {
    if (!pressedKey || index < 0 || index >= items.length) return null;
    const item = items[index];
    return item.key === pressedKey ? item : null;
}
// Application activation reuses its last committed window; cycling is separate.
function activationWindow(matches, active, history) {
    if (matches.includes(active)) return active;
    for (const window of history)
        if (matches.includes(window)) return window;
    return matches.length ? matches[0] : null;
}

// Authored controls have stable identifiers; provider-owned dynamic rows remain
// discoverable within their page rather than receiving fabricated static IDs.
const controls = [
    {"id":"group-window-presentation","page":"Desktop & Dock","label":"Application windows in a group","target":"settings-control-group-window-presentation"},
    {"id":"minimized-window-destination","page":"Desktop & Dock","label":"Minimized window destination","target":"settings-control-minimized-window-destination"},
    {"id":"separate-display-spaces","page":"Desktop & Dock","label":"Separate display spaces","target":"settings-control-separate-display-spaces"},
    {"id":"main-display","page":"Desktop & Dock","label":"Main display","target":"settings-control-main-display"},
    {
        "id": "show-files-on-the-desktop",
        "page": "Desktop & Dock",
        "label": "Show files on the desktop",
        "target": "settings-control-show-files-on-the-desktop"
    },
    {
        "id": "sort-desktop-files",
        "page": "Desktop & Dock",
        "label": "Sort desktop files",
        "target": "settings-control-sort-desktop-files"
    },
    {
        "id": "descending-desktop-order",
        "page": "Desktop & Dock",
        "label": "Descending desktop order",
        "target": "settings-control-descending-desktop-order"
    },
    {
        "id": "leave-margins-around-tiled-windows",
        "page": "Desktop & Dock",
        "label": "Leave margins around tiled windows",
        "target": "settings-control-leave-margins-around-tiled-windows"
    },
    {
        "id": "drag-windows-to-screen-edges-to-tile",
        "page": "Desktop & Dock",
        "label": "Drag windows to screen edges to tile",
        "target": "settings-control-drag-windows-to-screen-edges-to-tile"
    },
    {
        "id": "hold-option-while-dragging-to-tile",
        "page": "Desktop & Dock",
        "label": "Hold Option while dragging to tile",
        "target": "settings-control-hold-option-while-dragging-to-tile"
    },
    {
        "id": "drag-windows-to-the-menu-bar-to-fill-the-screen",
        "page": "Desktop & Dock",
        "label": "Drag windows to the menu bar to fill the screen",
        "target": "settings-control-drag-windows-to-the-menu-bar-to-fill-the-screen"
    },
    {
        "id": "hold-a-window-at-the-top-edge-to-open-overview",
        "page": "Desktop & Dock",
        "label": "Hold a window at the top edge to open Overview",
        "target": "settings-control-hold-a-window-at-the-top-edge-to-open-overview"
    },
    {
        "id": "tile-gap",
        "page": "Desktop & Dock",
        "label": "Tile gap",
        "target": "settings-control-tile-gap"
    },
    {
        "id": "remember-owned-window-geometry",
        "page": "Desktop & Dock",
        "label": "Remember Settings and Calendar window placement",
        "target": "settings-control-remember-owned-window-geometry"
    },
    {
        "id": "title-bar-double-click-action",
        "page": "Desktop & Dock",
        "label": "Title bar double-click action",
        "target": "settings-control-title-bar-double-click-action"
    },
    {
        "id": "dock-position",
        "page": "Desktop & Dock",
        "label": "Dock position",
        "target": "settings-control-dock-position"
    },
    {
        "id": "dock-size",
        "page": "Desktop & Dock",
        "label": "Dock size",
        "target": "settings-control-dock-size"
    },
    {
        "id": "automatically-hide-the-dock",
        "page": "Desktop & Dock",
        "label": "Automatically hide the Dock",
        "target": "settings-control-automatically-hide-the-dock"
    },
    {
        "id": "add-file-to-dock",
        "page": "Desktop & Dock",
        "label": "Add file to Dock",
        "target": "settings-control-add-file-to-dock"
    },
    {
        "id": "add-folder-to-dock",
        "page": "Desktop & Dock",
        "label": "Add folder to Dock",
        "target": "settings-control-add-folder-to-dock"
    },
    {
        "id": "show-scroll-bars",
        "page": "Desktop & Dock",
        "label": "Show scroll bars",
        "target": "settings-control-show-scroll-bars"
    },
    {
        "id": "sidebar-icon-size",
        "page": "Desktop & Dock",
        "label": "Sidebar icon size",
        "target": "settings-control-sidebar-icon-size"
    },
    {
        "id": "show-recent-applications",
        "page": "Desktop & Dock",
        "label": "Show recent applications",
        "target": "settings-control-show-recent-applications"
    },
    {
        "id": "show-indicators-for-open-applications",
        "page": "Desktop & Dock",
        "label": "Show indicators for open applications",
        "target": "settings-control-show-indicators-for-open-applications"
    },
    {
        "id": "magnification",
        "page": "Desktop & Dock",
        "label": "Magnification",
        "target": "settings-control-magnification"
    },
    {
        "id": "interface-text-size",
        "page": "Accessibility",
        "label": "Interface text size",
        "target": "settings-text-size"
    },
    {
        "id": "increase-contrast",
        "page": "Accessibility",
        "label": "Increase contrast",
        "target": "settings-increase-contrast"
    },
    {
        "id": "reduce-transparency",
        "page": "Accessibility",
        "label": "Reduce transparency",
        "target": "settings-control-reduce-transparency"
    },
    {
        "id": "reduce-motion",
        "page": "Accessibility",
        "label": "Reduce motion",
        "target": "settings-reduce-motion"
    },
    {
        "id": "mute",
        "page": "Sound",
        "label": "Mute",
        "target": "settings-control-mute"
    },
    {
        "id": "advanced-sound-settings",
        "page": "Sound",
        "label": "Advanced Sound Settings\u2026",
        "target": "settings-control-advanced-sound-settings"
    },
    {
        "id": "show-notification-previews",
        "page": "Notifications",
        "label": "Show notification previews",
        "target": "settings-control-show-notification-previews"
    },
    {
        "id": "do-not-disturb",
        "page": "Notifications",
        "label": "Do Not Disturb",
        "target": "settings-control-do-not-disturb"
    },
    {
        "id": "open-notification-centre",
        "page": "Notifications",
        "label": "Open Notification Centre",
        "target": "settings-control-open-notification-centre"
    },
    {
        "id": "save-recent-search-queries",
        "page": "Search",
        "label": "Save recent search queries",
        "target": "settings-control-save-recent-search-queries"
    },
    {
        "id": "clear-search-history",
        "page": "Search",
        "label": "Clear Search History",
        "target": "settings-control-clear-search-history"
    },
    {
        "id": "disable-clipboard-history",
        "page": "Search",
        "label": "Disable Clipboard History",
        "target": "settings-control-disable-clipboard-history"
    },
    {
        "id": "clear-clipboard-history",
        "page": "Search",
        "label": "Clear Clipboard History",
        "target": "settings-control-clear-clipboard-history"
    },
    {
        "id": "open-search",
        "page": "Search",
        "label": "Open Search",
        "target": "settings-control-open-search"
    },
    {
        "id": "refresh-file-index",
        "page": "Search",
        "label": "Refresh file index",
        "target": "settings-control-refresh-file-index"
    },
    {
        "id": "accent-colour",
        "page": "Appearance",
        "label": "Accent colour",
        "target": "settings-accent-colour"
    },
    {
        "id": "icon-style",
        "page": "Appearance",
        "label": "Icon style",
        "target": "settings-icon-style-card"
    },
    {
        "id": "start-after-inactivity",
        "page": "Screen Saver",
        "label": "Start after inactivity",
        "target": "screen-saver-enabled"
    },
    {
        "id": "screen-saver-idle-delay",
        "page": "Screen Saver",
        "label": "Screen saver idle delay",
        "target": "screen-saver-delay"
    },
    {
        "id": "animate-shapes",
        "page": "Screen Saver",
        "label": "Animate shapes",
        "target": "screen-saver-motion"
    },
    {
        "id": "preview-screen-saver",
        "page": "Screen Saver",
        "label": "Preview screen saver",
        "target": "screen-saver-preview"
    },
    {
        "id": "wallpaper-scope",
        "page": "Wallpaper",
        "label": "Wallpaper scope",
        "target": "wallpaper-scope"
    },
    {
        "id": "choose-image",
        "page": "Wallpaper",
        "label": "Choose image\u2026",
        "target": "wallpaper-choose"
    },
    {
        "id": "input-device",
        "page": "Input Devices",
        "label": "Input device",
        "target": "input-device-selector"
    },
    {
        "id": "menu-bar-visibility",
        "page": "Desktop & Dock",
        "label": "Menu bar visibility",
        "target": "menu-visibility"
    },
    {
        "id": "configured-shortcuts",
        "page": "Accessibility",
        "label": "Configured Shortcuts",
        "target": "settings-configured-shortcuts"
    },
    {
        "id": "appearance-light",
        "page": "Appearance",
        "label": "Light",
        "target": "settings-appearance-light",
        "terms": ""
    },
    {
        "id": "appearance-dark",
        "page": "Appearance",
        "label": "Dark",
        "target": "settings-appearance-dark",
        "terms": ""
    },
    {
        "id": "appearance-auto",
        "page": "Appearance",
        "label": "Auto",
        "target": "settings-appearance-auto",
        "terms": "automatic appearance"
    }
];
function control(id) {return controls.find(row=>row.id===id) || null;}
function destination(page,id,pages) {
    if(!pages.some(row=>row.name===page))return null;
    const entry=id?control(id):null;
    if(id && (!entry || entry.page!==page))return null;
    return {page:page,control:id || ""};
}
function results(query,pages) {
    const terms=String(query || "").trim().toLowerCase().split(/\s+/).filter(Boolean);
    if(!terms.length)return [];
    const matches=text=>terms.every(term=>text.toLowerCase().includes(term));
    const found=controls.filter(row=>matches(row.label+" "+row.page+" "+(row.terms || ""))).map(row=>({page:row.page,control:row.id,label:row.label,name:row.page}));
    for(const page of pages)if(matches(page.name+" "+page.terms))found.push({page:page.name,control:"",label:page.name,name:page.name,icon:page.icon,color:page.color});
    const exact=String(query).trim().toLowerCase();
    return found.sort((a,b)=>Number(b.label.toLowerCase()===exact)-Number(a.label.toLowerCase()===exact));
}

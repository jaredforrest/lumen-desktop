# Files tabs and sidebar layout

Files tabs have stable session identities. Reordering a tab preserves its folder,
back/forward history, filter, view mode and selected paths. Closing an inactive
tab leaves the current view alone; closing the active tab restores the adjacent
tab's saved state without adding a new history entry. The last tab closes the
Files window. Tabs represent folder views, not editable document buffers; this
does not implement dirty-document tab confirmation for other applications.

Drag a tab to reorder it, or focus it and press Control+Shift+Left/Right. Escape
cancels a held drag, including later motion before release. A release outside the
tab strip cancels. The close button names the exact tab; Control+W closes the
focused tab. Existing Control+Tab and Control+Shift+Tab switch views. A long tab
strip scrolls instead of shrinking labels and close targets indefinitely.

Favourites and Locations each have a separate disclosure button. Collapsing a
group preserves its destinations and current folder/file selection. The add
button remains a distinct action. Resize the sidebar at its divider, or focus the
divider and use Left/Right (10 pixels), Home or End. Escape cancels a held resize.
The requested width and collapsed groups are validated file-view preferences.
Narrow windows temporarily clamp the displayed width to leave at least 300
pixels for content; expanding the window restores the wider requested width.

Reusable controls are `DocumentTabs`, `SidebarGroup` and `SidebarResizeHandle`.
Immutable tab operations and width constraints live in separate pure models.
Changing these controls must not dispatch folder navigation implicitly.

## Verification

The backend/unit suite includes tab identity, snapshot isolation, preference
validation and real Qt/backend restoration tests. The native input fixture uses
a disposable home/state and isolated headless compositor:

```sh
nix build .#shell .#compositor-basic .#test-pointer
nix develop --command desktop-test-python tests/desktop/files_layout_input.py \
  --launcher /path/to/shell/bin/lumen-desktop-shell \
  --sway /path/to/compositor/bin/sway \
  --pointer-tool /path/to/pointer/bin/wlrctl --output /tmp/files-layout-input
```

It checks actual pointer and keyboard disclosure, resizing, cancellation, tab
reorder/close, saved history/selection and 480-pixel window bounds. Screenshots
must contain painted file content. Original disposable file bytes are checked
at the end. No live desktop inputs, files or accounts are used.

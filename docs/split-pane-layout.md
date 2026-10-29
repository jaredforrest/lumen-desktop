# Resizable file panes

Sidebar, preview and column dividers share `SplitResizeHandle`. Pointer motion
previews the split; release commits it, while Escape or pointer cancellation
restores the previous preference. Left/Right move the divider, Home/End choose
its bounds, and accessible increment/decrement adjust the named pane width.
Preview uses the opposite direction because it is the trailing pane.

Preview and column preferences join the existing Files preference document.
Preview width is an integer between 160 and 600 logical pixels; up to sixteen
column-slot widths have the same validated storage range. Slots identify visual
column positions, not document paths. Invalid preference edits preserve the old
file, and partial updates preserve unrelated preferences. Consumer policy owns
persistence of the existing session-state directory.

Rendering clamps the preferred sizes to current output capacity and enlarged
text minimums without rewriting the saved widths. A narrow window temporarily
hides an already requested preview and explains why; widening restores it.
Columns retain their readable minimum and scroll horizontally. Unresizable panes
have no misleading drag target; the column resize target has its own space beside
the list scrollbar. Current pane minimums are derived design choices rather than
reference-system measurements.

The layout tests check pane minima, narrow/wide capacity and reversible clamps
at all supported text scales. Preference tests exercise the actual storage
handler, round trips, rejected malformed values and preservation of prior bytes.
`files_layout_input.py --split-panes` exercises physical divider and keyboard
input, cancelled held gestures, unchanged file selection, actual fractional output
scale, narrow-window fallback and a fresh backend/window preference reopen in
the full disposable namespace on both compositor variants.

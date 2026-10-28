# Logical desktop lifecycle

Logical identity is a persistent UUID, separate from native workspace ID, native
name, display label and order. Explicitly created desktops remain in the catalog
when empty. An empty desktop stays dormant until selected or given a window;
creation does not change focus or create a placeholder application. Native names
are opaque and never renamed to simulate order. Labels remain readable and stable.
Existing native desktops are adopted without claiming that every historical empty
workspace is a persistent user-created desktop. The catalog is bounded to 64.

Overview offers creation on a connected display, selection, explicit removal with
a surviving destination, and manual order. The destination picker and assignment
choices use the logical catalog rather than an invented fixed set of numbered
desktops. Native names are used only at the compositor boundary.

## Order and cancellation

Pointer motion and keyboard previews are local. Release or Return commits the
captured same-display permutation; Escape, pointer cancellation, hiding Overview,
a display change or a changed policy revision cancels it. Every desktop on that
display must occur exactly once. Other displays are untouched. A manual commit
explicitly disables recent-use ordering, as the preview explains. Re-enabling
recent-use ordering selects that policy without changing logical IDs or labels.

Policy revisions and the owned compositor socket epoch reject stale mutations.
A status refresh with no actual policy change does not invalidate a gesture.
An unavailable output rejects create/order/removal. On disconnect, an empty
persistent desktop adopts a surviving reachable output; its UUID stays unchanged.
Live native workspace placement follows actual compositor output migration. With
no connected output, desktops remain recorded but unavailable until reconnection.

## Removal and saved placements

Removal names exact source and destination logical IDs from the captured catalog.
They must be distinct ordinary desktops on the same connected display. This also
prevents deleting the last desktop on that display. The confirmation explains
that windows move and nothing is closed.

The backend captures actual container IDs and PIDs. Before each move it rechecks
that exact window and its source membership. It never selects by title, closes a
window, kills a process or substitutes another document. If a captured window has
already moved away, it stays there. A newly arriving source window prevents
retirement rather than being silently ignored. A failed evacuation may leave
already moved windows in the survivor; the source stays recorded and the error
explains that the moved windows were preserved.

The native `desktop_rehome` extension exposes every saved workspace association
and provides `lumen_window rehome SOURCE TARGET OUTPUT`. This updates matching
minimized, filled, hidden and show-desktop snapshots without revealing a hidden
window. Saved output identity allows later restore to recreate a dormant target
on its output even when another display is focused. Layout-parent references are
cleared when moving saved placement to another desktop. Removal is disabled until the running compositor advertises this extension;
installing its package alone does not change an already running compositor.
An empty source still
visible on its output is replaced there by the survivor; focus on another output
is restored when possible. Only after fresh native queries show no remaining
source or saved association is its logical record retired.

Assignments to a retired UUID stay explicitly unavailable instead of silently
being reassigned to a different desktop. Partial native operations cannot be
rolled into a fake atomic result: this is a checked sequential IPC transaction,
not an atomic compositor desktop broker.

Active full screen and paired placement are separate from ordinary desktop
removal. The backend requires leaving those modes before retiring a desktop used
by their saved placement. Native pair metadata reports its exact restore desktop,
so unrelated active pairs do not block ordinary removal. Older backends without
that metadata fail conservatively. All-desktop membership and stage groups are
still unavailable; this feature does not simulate them.

## Tests

Rust Spaces tests exercise persistence, stale revisions, output constraints and
assignment validation. The native fixture covers desktop evacuation.
`javascript_models_fixture.qml` checks immutable previews and stale/cancelled commits.
`tests/desktop/desktop-policy-rust` runs a private compositor and real native windows on
both backends: visible/hidden/minimized evacuation, fully minimized removal,
saved-output restoration, disconnected output recovery and production Overview
pointer/keyboard commit/cancel/confirmation controls. It preserves exact process
and container identities. All fixture processes, windows, documents and session
state are disposable; the live desktop is never used.

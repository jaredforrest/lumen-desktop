# Window Groups

Window Groups is an optional presentation mode, initially off for every new
compositor session. Open **Window Overview → Window Groups** to enable it,
select a group, add the selected Overview window to a group, or use **New Group**
to detach that window into its own group. Removing the last member retires an
empty group automatically. Existing independent native windows initially receive
individual groups. Group selection and recent-use counters are separate from
logical desktop identity and desktop ordering.

The compositor owns membership and a separate scene visibility reason. It never
moves a window to the scratchpad to implement group hiding. Disabling the mode
therefore removes only that reason: normal geometry, layout membership,
application hiding, minimisation and Show Desktop restoration stay intact. A
native focus request selects its target's group. Actual native xdg parented
windows follow their owner; popup menus are dismissed when their group leaves.
No document window is closed or process terminated by these commands.

Membership uses native container IDs and PIDs. Membership edits preserve unrelated
shell control focus; explicit group selection activates a native member. Every mutation requires the exact
native revision; the Overview backend additionally checks its captured compositor
socket epoch. Buttons capture targets when pressed and commit only when clicked;
changing membership before release rejects the stale operation. IDs and state are
session-local and survive shell observation restarts through native GET_TREE;
they are not restored across compositor restarts. No application name or title
confers action authority.

Groups belong to one actual native desktop at a time. Moving a window to another
desktop reconciles its membership there; native desktop migration after an output
disconnect preserves the live identity. Cross-desktop additions are rejected.
Full screen, paired layout and Show Desktop suspend masking in the affected
desktop and reject membership edits. Native session lock rejects all mutations.
The implementation requires independently active desktops per display, which the
supported compositor provides; an adapter lacking that fact must reject enable.
Old compositors without the extension have no enabled controls or fallback
placement emulation.

## Current boundaries

This is a functional grouping workflow, not a complete spatial presentation.
The recent groups edge selector is implemented below. Pointer drag into/out of
that selector, packing, previews and animations are not implemented. Ordinary tiled windows
retain their original layout slots. There is no together/one-window-at-a-time
setting yet. Sticky windows, X11 windows and shared-process shell windows are currently ungrouped;
the latter need explicit owned-role adapters rather than title-based inference.
Groups have a 256-entry bound; additional eligible roots remain ungrouped and
visible if that limit is reached. A native desktop destroyed while all its members
are conventionally hidden loses its group definitions; ordinary hide/restore
placements remain intact. No cross-session group persistence or all-desktop
membership is claimed.

## Native interface and verification

`get_version.native_window_groups` declares the extension. GET_TREE's root
`window_groups` object has version 1, enabled/locked/separate_displays facts,
revision and exact groups, members, recent counters and suspended states.
`lumen_state.group_hidden` is a separate observed boolean; older observers report
unknown rather than infer it from scratchpad membership.

Commands are `lumen_window groups REV on|off`, `... REV select GROUP`, and
`... REV add|remove GROUP CONTAINER PID`. Remove creates a singleton group and
selects it. These are native atomic membership edits, independent from normal
placement commands. The backend constructs them exclusively from typed numeric
captured identities.

`test_window_groups.py` checks strict types, duplicate and conflicting facts,
stale targets, independent hide semantics, unsupported adapters and lock/suspend
rejection. `window_groups_native.py` uses real disposable native windows, keyboard,
pointer, paired/fullscreen presentation, session lock, output disconnect and
clean shutdown on both compositor variants. `desktop_lifecycle_input.py` also
exercises the production dialog with actual private pointer and keyboard input.
No fixture contacts the host session or user documents.

## Recent-group edge selector

While Window Groups is enabled, each display has a concealed selector at its
left edge. Hover at the edge to reveal recent groups on that display's currently
visible native desktop; moving away conceals it. The cards are a recent-use list,
not document thumbnails. Activating a card by pointer selects its captured group
on release. Press Escape during a held press, release outside the card, or click
outside a keyboard-open selector to cancel without changing group membership.

**Super + Control + G** opens keyboard selection on the native focused display.
Up/Down and Home/End preview rows; Return or Space activates; Escape cancels.
The shortcut is declared in the shared registry and can be overridden by the
consumer. Disabled grouping, unsupported native capability, suspended presentation,
lock, stale observation and disconnected outputs provide no active selector.
Unlock does not reopen it. The selector coordinates with existing shell popups.

The selector requires `get_version.native_window_group_selector`; old native
group builds provide no fallback selector. Every press captures group, native
revision, compositor epoch, display and visible desktop. A fresh backend query validates those facts, and native `select-scoped`
checks output/visible-desktop ownership again atomically before selection. A
workspace switch between validation and command cannot redirect activation to a
hidden desktop. Native groups remain the sole visibility and membership owner;
edge previews never change them.

The edge width, reveal/leave delays and card dimensions are chosen implementation
policy, not measured reference geometry. Pointer grouping drag, live previews,
packing and presentation animation remain unsupported. Acceptance uses both
native compositor variants, two real disposable outputs, real keyboard/pointer
input, actual session lock and observer shutdown. It checks MRU/scope projection,
preview cancellation, pointer release, stale native scope rejection and removal
of disconnected output surfaces.

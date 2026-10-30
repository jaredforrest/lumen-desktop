# Guarded desktop transfer

Overview supports dragging an ordinary window card onto a logical desktop tile.
The accessible **Desktop…** handle offers the same transfer; an ordinary card
click still activates its window. Escape, release outside a target, disabling the
card, closing Overview or locking cancels the offer. A drag captures its source
and destinations before dispatch. A rejected or uncertain transfer is never
silently recaptured, replayed or retried.

## Supported movement

An independent ordinary window may move between desktops and outputs without
activating the destination desktop. This includes the desktop's own ordinary
Files window. A visible Window Group member may move: native reconciliation
retains the old partners and their placement and gives the moved root its normal
destination membership. It does not force grouping off or replace the selected
member of an existing destination group.

A floating or tiled root may carry up to 15 ordinary floating, nonmodal descendants
(16 windows including the root). Every descendant must have an exact supported
parent relationship and the same original workspace and output. Floating child
sizes and offsets follow the root's actual movement; a tiled root can be resized
by the destination layout. The entire bounded family is validated before any
movement. Dragging a descendant alone is unavailable.

Active modal or unknown relationships, fullscreen ancestors, fullscreen destination
containers (including non-view splits), hidden or minimized windows, saved restore
placement, paired layouts or pair-owned dialogs, sticky placement, nonfloating
children, cross-origin families and oversized or cyclic families are refused.
Every family member must have no saved desktop/display assignment. Malformed
assignment records fail closed. Managed X11 transfer uses the same captured path
with validated native relationships and XRes ownership; unknown ownership is
unavailable. An unsupported raw transient edge can refuse a misleading family
offer, but cannot authorize membership or movement. Enabled all-desktop
presentation on any family member must be explicitly cleared before transfer,
including native requests without a saved desktop assignment. These are explicit
eligibility limits, not claims about external reference measurements.

## Captured authority

The provider advertises `space_transfer_guard: 3`; missing capability disables the
offer. `LST3` is a typed, bounded binary packet, not a native command payload. It
binds the exact compositor session, allocation generation, source container/PID/
foreign-surface identity, source workspace/output/frame, destination identity or
captured absence, and a digest of the captured topology. IDs crossing the Qt
boundary must be positive safe integers. Packets are limited to 2048 bytes (4096
hex characters); topology is limited to 1 MiB, depth 64 and 8192 nodes.

The transfer projection clones IPC JSON and sorts only validated `floating_nodes`
arrays by unique native ID, so raising Overview does not invalidate an otherwise
unchanged drag. Membership, focus arrays, tiling order and policy facts remain
bound. A reserved frame field is overwritten from each published native frame,
including legitimate zero-sized unrelated windows; eligible family frames must
have positive dimensions. C and Rust share exact digest golden tests. The older
focus-guard codec retains its original ordering semantics.

One native callback validates the full packet, current topology, source and family,
destination and all exclusions before creating an empty target or moving windows.
Refusal leaves the native state unchanged. No title matching or application-name
inference establishes authority. Process witnesses are sampled; this does not
claim an atomic kernel process-lifetime guarantee. The UI also holds its existing
session/revision/lock policy and invalidates stale offers.

After a complete family moves, a seat focused on one of its members receives the
remaining source-desktop focus target. Unaffected focus, including Overview, stays
in place; a moved child cannot retain keyboard focus offscreen. Empty source
splits and workspaces retire after this repair, not midway through the family.
While a layer surface owns keyboard input, affected native return history is
repaired without taking input from the layer. Closing the layer returns to the
remaining origin view instead of reactivating the transferred family. Unaffected
native focus histories retain their existing order.

## Acceptance and limits

The native CI wrapper runs both compositor backends and two capability-disabled
providers in private namespaces. The accepted corpus covers natural card and
handle drops, click activation, Escape/outside/disabled cancellation, production
Files, grouped roots, floating and tiled families across outputs, stale geometry,
modal/hidden/lock/output-loss and absent-slot changes, malformed packets and full
state preservation on refusal. A causal old-provider run accepted a move into a
non-view fullscreen destination; the repaired providers reject that same boundary.

The managed X11 protocol corpus additionally covers exact family identities,
floating/tiled roles, visible and keyboard-focus workspaces, physical bystander
key delivery, safe retirement of an inactive last-leaf split, unknown XRes
ownership and fresh projection/cross-process family refusals. It shares the
production capture and packet implementation; physical card dragging remains a
separate UI acceptance scope.

The exclusive-layer corpus uses a real keyboard-exclusive layer and physical
keys before and after family movement. It checks exact family frames and native
states while input stays with the layer, then verifies the remaining origin view
receives the next key after layer closure and the destination remains inactive.

This feature does not implement spatial packing, live thumbnails or animation,
and does not assert behavior for unsupported providers or application protocols.

# Window context and document ownership

Keyboard focus, native parenthood, application presentation grouping and the
main document association are different observations.

The compositor advertises `native_window_relationships: true` and includes
`lumen_relationships` version 1 in each window. `parent_supported` reports whether
the native parent protocol can be observed. `parent` is the current mapped direct
parent container ID, or null. `modal` is the actual xdg-dialog state, or null when
that native window has no dialog object; supported X11 windows expose their
observed EWMH modal boolean. An absent extension is unknown, including for
older backends. A null parent does not assert a document role.

The standard xdg-dialog manager is instantiated on both supported backends.
Per-seat compositor focus redirects a parent target to its live modal descendant;
it does not block sibling documents or other applications. Plain utility parents
do not redirect focus. Ambiguous sibling modal requests refuse a parent focus
change rather than selecting an arbitrary child. Dynamic parent and modal signals
refresh workspace arrangement and apply the focus rule to current seats. Dialog
objects and mapped views have separately cleaned-up listener lifetimes.

The title-free runtime snapshot exposes parent/modal facts and a separate
`context_window`/`application_hint` derived from live native ancestry. Advertised
application IDs are not authenticated package identities. It never invents a main
document from parenthood, a title or most recent focus. `contexts` can accept
independent adapter declarations with live window IDs; these are not an external
command channel and grant no destructive document authority.

Owned Files windows explicitly register QObject references as main windows.
Quick Look and File Info register their actual owning Files instance as their
main association while independently becoming the key utility window. Returning
to another application clears the owned key recipient but keeps the document
association. Returning to a different Files window changes the main association;
returning to its utility restores that utility's explicit owner. Closing the owner
invalidates its main association. Files menu commands use this owning instance.
These annotations affect presentation and document routing within the owned app;
they do not authorize Force Quit, close-all or external process control.

The shell correlates the active native toplevel handle with the owned window's
actual Qt active state before retaining a presentation grouping association.
This lets an observed owned utility remain in its owning Files Dock/switcher group
when inactive. Title equality is only an additional race guard, not the source of
its role. Destroyed handles and invisible owners are excluded. The menu bridge
exposes parent application context separately from its exact key-window exporter;
its existing exporter PID and focused token guards remain unchanged.

## Verification and limits

Rust desktop-state tests cover independent main/key association, graph cycles,
ambiguous modal siblings, stale adapter declarations and strictly typed facts.
The Rust `window-relationships-rust` fixture uses a disposable real Wayland client and virtual
keyboard in an isolated compositor: utility/document/other-app recipients, modal
scope, dynamic set/unset/reparent/unparent, dialog-object destruction and live
orphan utility after parent exit. The Rust `owned-window-context-rust --full-shell` fixture
exercises real Files, Quick Look and inspector windows, routing a New Tab command
from a key utility to the associated document and retaining its application group.
CI runs native integration on both compositor backends.

Unreported external application main documents remain unknown. Native parenthood
does not supply document-command semantics, dirty state or an application menu.
Native popup surface introspection is not implemented by this extension. Toolkit popup surfaces are not separate managed
toplevels and do not create a new application context here. The runtime remains a
sequential read-only observation rather than an atomic focus broker. Owned app
associations are in-memory and are rebuilt by actual owned window focus after a
shell restart; no stale QObject or container identity is persisted.

## X11 relationship capability

Managed Xwayland windows use the current server-observed `WM_TRANSIENT_FOR` and
`_NET_WM_STATE_MODAL` facts. Ancestry is supported only when each live mapped
parent and child has the same positive XRes local-client PID. Unknown PID,
cross-process parent, override-redirect/unmanaged surface, unmapped parent,
cycles and excessive ancestry fail
unavailable. A null supported parent means no current mapped parent, not a main
document role. `_NET_WM_PID`, class and title hints never establish this policy.
X11 clients share their server's trust domain; these observations do not
authenticate an application, its executable, its documents or other clients'
behavior. They authorize no process termination or document command.

Both compositor variants and the effects renderer use the same aligned wlroots
build. Its bounded extra `set_modal` signal reports actual modal transitions
from EWMH messages and property replacement/deletion; it has the same signal
initialization, observer teardown and empty-list destruction checks as the
existing parent signal. Parent and modal listeners recompute branch focus and
placement. Parentless modal windows and transient windows cannot be primary
paired members. Pair dialog ownership uses the same live relationship policy
rather than a mutable class or an unsupported cross-process hint.

`tests/desktop/xwayland-relationships-rust` uses a Rust 2024 driver, a fixed
native libxcb client, and an actual
private Xwayland server. It checks utility/document/other-process physical key
recipients, live map/reparent/unmap/destroy and replacement, modal message and
property transitions, cycle rejection, ambiguous modal siblings, parentless
modal pair refusal, forged PID hints, a managed child referencing an unmanaged
parent through manageability transitions, and clean exit with live modal clients.
The separate `--without-xres` case really disables the server extension and
checks unknown-PID behavior. Both cases run on both compositor variants. No host
X server, input, applications or filesystem is involved.

## Preferred-size zoom

The desktop's window-geometry adapter can explicitly register a preferred client
width and height with `lumen_window preferred-size W H`. Desktop-owned
`WindowGeometry` instances supply their declared implicit dimensions. The helper
captures its real parent PID and start time, compositor socket identity and one
mapped container ID; it rechecks them before dispatch with exact native criteria.
This is a trusted same-user integration contract, not an authenticated IPC channel
or a guarantee that arbitrary clients advertise useful preferred content sizes.
The compositor never guesses preferred dimensions from titles or initial map size.

`lumen_window zoom` and the `zoom` title-bar double-click preference toggle between
that explicit preferred size and the original native placement snapshot. The
native work area and client minimum/maximum constraints bound the result. Fill
and directional tiling replace zoom mode while retaining the original restore
snapshot. Minimise, hide and full screen keep their existing placement lifecycle.
Zoom is unavailable for unregistered, transient/modal, full-screen, paired,
unmapped and hidden windows, or when client constraints cannot fit the work area.
An independently visible member of a window group can zoom itself.

The Rust 2024 `lumen-preferred-zoom-fixture` exercises registration, native
Zoom and restore, title-bar gestures, output shrink, protocol minimums, foreign
window refusal and rapid stale acknowledgements in private basic and effects
compositors. Both release runs pass 19 ordered checks.

Native `lumen_state.zoomed` identifies the mode; `preferred_zoom` reports current
eligibility. The title-free runtime reports these explicit facts and rejects
inconsistent zoom state without a native restore snapshot. Older compositors do
not advertise `native_preferred_zoom`; the geometry adapter continues fitting
windows without registering unsupported metadata. Newly installed native actions
require the new compositor to be running; package activation alone does not
replace the existing compositor process.

Explicit filled/zoomed Restore waits for acknowledgement of the native configure
that represents the current requested client size. Older client commits still
update the observed rendering geometry and complete their own transactions, but
cannot rewrite the newer pending restored rectangle. A queued native resize
invalidates an older acknowledgement tag; the later matching transaction tags
its own size and wrap-safe serial. The guard also accepts a later serial after a
transaction timeout. It does not extend transaction deadlines or require a client
to commit an exact size before later legitimate self-resizing is allowed.

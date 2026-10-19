# Runtime observation and transport primitives

`lumen-desktop-state` is a read-only compositor observer. It validates the owned
local compositor socket and emits window IDs scoped to an endpoint epoch, output
bounds, workspace membership, key-window state and observed frame ownership.
It omits window titles, PIDs, environment and document contents. It opens no
listener, grants no privilege and performs no window actions.

The snapshot explicitly identifies its limitations: separate compositor IPC
queries are not an atomic revisioned snapshot; app IDs are client-advertised
hints; app residency, main-document ownership, hidden state and menu export
require additional app/backend capabilities. Scratchpad membership alone is not
proof of minimisation. Missing information is null or an unavailable capability,
never fabricated from a title or list order. Namespaced placement fields are
used only when actually supplied by a compatible compositor.

Run `lumen-desktop-state --fixture /absolute/synthetic-state.json` for an offline
fixture containing `tree` and `outputs`. Production diagnostics should retain only
synthetic or appropriately private artifacts. The command's JSON has schema
version 1 and must not be confused with a complete application registry.

`res/lumen-desktop/runtime/protocol.py` supplies dependency-free primitives for
future adapters: bounded four-byte-length UTF-8 JSON frames, duplicate-key and
nonfinite-number rejection, fragmented frame decoding, peer/epoch-scoped replay
fingerprints and revision-gap reconciliation. These primitives are not a running
command broker and do not claim authentication or capabilities an adapter has
not implemented. Replay retention fails closed at capacity; uncertain destructive
requests require reconciliation after a process restart.

Regression tests use synthetic streams, fake monotonic time, isolated socket
paths and compositor trees. They cover changed-payload retries, bounded allocation,
sequence/epoch gaps, duplicate identities, unknown minimisation and title privacy.
No test reaches a real account, modifies a document or injects live input.

## Opt-in clipboard history

`lumen-clipboard-history` provides status, list/search, explicit enable/disable,
clear and copy-by-ID. It never pastes into another application. Collection begins
only after the user accepts the warning and enables the installed integration.
The module integration is off by default and requires an explicit consumer
retention and item bound. Starting an overlay only reads status; it cannot start
collection. A failed collector start rolls consent back instead of showing success.

Only consent persists. Entries live under the private user runtime directory,
expire by monotonic age, are bounded, and disappear at logout. Disabled/expired/
cleared entries cannot be copied by stale ID. Marked sensitive clipboard offers
are excluded. Arbitrary unmarked secrets cannot reliably be recognised, so the
consent interface explains this limit and provides Clear and Disable. Tests use
synthetic text and injected writers; no regression captures the real clipboard.

### Focus profiles

`lumen-notifications focus` reports profiles and active policy. Control Centre
provides profile selection, one-hour activation, Off and Use schedules. Profiles
are local user state in the notification service's private state directory.
`lumen-notifications focus work 3600` activates Work for one hour; `off` overrides
schedules, while `auto` restores schedule evaluation. Activation without a duration
continues until explicitly changed. Temporary activation ends on either its wall
clock deadline or suspend-aware elapsed deadline, and is reevaluated after restart.
Overlapping schedules select the first profile in stored order. Schedule weekday
numbers are Monday=0 through Sunday=6, with start/end minutes in local civil time;
overnight intervals belong to their starting weekday. Clock and timezone changes
recompute schedules on the next service tick. An expired temporary override returns
to schedules, so an independently active scheduled profile can remain visible.

Configure exceptions and schedules without editing generated files by providing
one JSON object on standard input to `lumen-notifications focus configure`:

```json
{"id":"work","name":"Work","allowedApps":["calendar"],"allowCritical":false,"schedules":[{"days":[0,1,2,3,4],"start":540,"end":1020}]}
```

Application exception identities are the desktop-entry hint, falling back to the
notification application name. They are client-supplied metadata, not authenticated
application identities. Focus controls presentation, not a security boundary.
Allowed notifications still respect the app's silent/off policy. Silenced items
remain in Notification Centre; disabling Focus never replays old banners. People
exceptions are unavailable because the notification protocol supplies no reliable
sender identity. No cloud synchronization is implied.

## Shell action routing and Dock preferences

Desktop entry launchers and compositor actions resolve the current
`lumen-desktop-shell.service` MainPID before calling Quickshell IPC. They do not
select a shell using the build-specific asset path: a configuration reload and a
shell service restart can happen separately during deployment. Invalid, absent,
or differently owned service PIDs fail closed. An action is not replayed if the
selected shell exits during dispatch. The integration test replaces the shell
from a different asset directory while leaving the compositor binding unchanged.

`appearance.dockPins` is either null (the supplied working application pins) or
an ordered list of installed desktop-entry IDs. The fixed navigation slots are
outside that list. Context commands keep/remove/move an application alias;
removing an alias does not uninstall the app or close its windows. Pointer drag
inserts an app before a pinned target; Escape and release outside cancel it.
Pin writes use the same authoritative settings transaction as Settings.
`dockIndicators` controls observed-window indicators. These are not a claim of
complete process residency: applications with no observable windows still need
a resident application authority. `dockRecentApps` displays up to three recent
unpinned, no-longer-visible applications after running apps. Recent IDs are kept
for the current shell session; they are resolved against current desktop entries
and never retain destroyed entry objects.

### Notification preview privacy and control layout

Notification preview `never` redacts subject, body, inline images and action labels
before shell transport, including accessible UI names. App attribution and history
remain. `always` restores previews; the authoritative preference is reloaded live.
Lock-screen content is supplied by the locker, which does not render notification
content. A conditional unlocked-only preview option is intentionally unavailable
until the notification service has an authoritative lock-state channel.

Control Centre's Edit controls view changes shown controls and their order through
the same validated Nix preference transaction as System Settings. Hidden controls
remain in the complete order inventory and can be restored. Control availability
continues to reflect hardware; editing cannot manufacture an unavailable device.
Pending edits are serialized and coalesced per preference key.

Media keys select real MPRIS players, prefer a playing player, check the requested
capability and treat an empty session as a successful no-op. Relay-only endpoints
are excluded. The shell likewise does not use relay-only media items; consumers
should not run a synthetic relay alongside direct MPRIS discovery because its
empty-state property errors originate before QML filtering.

### Built-in desktop widgets

Edit Widgets in Notification Centre controls the built-in Calendar's separate
centre and desktop layouts. Desktop display is opt-in, with month/agenda or compact
agenda, full colour, monochrome or automatic colouring. Automatic colouring dims
and desaturates the calendar when a regular window owns application focus. A
bottom-layer, input-masked surface keeps the rest of the desktop click-through.
Drag the handle revealed on hover to move the calendar; normalized per-output positions
persist and clamp to available bounds when output geometry changes. Month and
agenda controls open the same associated Calendar application as the centre.
The shared widget state preserves centre preferences when desktop placement changes.

Only the built-in Calendar provider is implemented. A general third-party widget
provider protocol, additional provider catalogue and unrestricted resize geometry
remain implementation work. No proprietary widget interoperability is claimed.

Folder aliases in `appearance.dockFolders` carry an absolute path, sort order,
direction and list/grid/automatic presentation. Folder requests carry a
monotonically increasing ID; a reply for an older folder or dismissed stack
cannot replace its rows. The list is bounded by the file service scan limit and
remains scrollable; truncated results expose Open in Files. Opening an item uses
its captured path/URI rather than an asynchronously replaced index.

Dock file drops accept copy semantics only and never move the source. A GIO
backend validates the entire set (local, readable regular files; selected
installed app; declared content-type compatibility) before one launch call.
Unknown compatibility fails with a visible error instead of guessing. Launches
run outside the shell service cgroup. This does not implement file aliases or
folder copies within the Dock.

`dockPosition`, `dockSize` and `dockAutoHide` control edge placement and shelf
geometry. Automatic hiding releases reserved work area and retains a narrow
edge input region. Keyboard focus reveals the Dock. The pointer fixture keeps
its private virtual pointer alive while checking hover; destroying the only
headless pointer clears Qt hover and cannot represent a physical user pointer.

### Constrained menu bar

Application identity and the system menu remain on the left; the clock and Control
Centre remain on the right. Heading widths use the current font's actual advance
metrics. Excess application headings move into More menus and status controls;
optional Sound, Focus and Search status items also move there as space narrows.
Arrow keys, Return and Escape operate this overflow with the same menu target
identity checks as the normal menu. This is an explicit narrow-screen adaptation,
not a claim of an identical reference overflow presentation. Software-rendered
sessions use canvas status glyphs and plain surfaces instead of unsupported shader
effects, preserving visible controls rather than merely valid scene geometry.

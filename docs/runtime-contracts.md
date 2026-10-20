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
collection. Explicit first enable waits for the observer's initial-event acknowledgement;
a failed user-service or unsupported provider rolls that initial consent back.
`enabled` reports consent separately from `collecting`. Availability is backed by
private runtime readiness metadata with exact supervisor/native PID start-time
identities and the current consent epoch. A background disconnect/restart retains
valid consent and reports collection unavailable rather than pretending it is
active. Readiness files contain process facts, never clipboard text or hashes.

Only consent persists. Entries live under the private user runtime directory,
expire by monotonic age, are bounded, and disappear at logout. Disabled/expired/
cleared entries cannot be copied by stale ID. The native data-control observer
skips the initial selection event (including an empty selection), then reads
future `text/plain;charset=utf-8` or `text/plain` offers, preferring explicit
UTF-8 plaintext independent of MIME advertisement order. Rich-only offers are
unsupported; no HTML conversion is fabricated. It checks the exact offer's `x-kde-passwordManagerHint` and
`application/x-keepassxc` MIME markers before requesting payload data. Marked
history exclusions do not prevent the destination application from independently
reading the ordinary clipboard. Arbitrary unmarked secrets cannot reliably be
recognised, so the consent interface explains this limit and provides Clear and
Disable.

Transfers are limited to 256 KiB and three suspend-aware BOOTTIME seconds. A replaced unfinished offer
is discarded. Clear and permission transitions rotate a private runtime epoch and CLOCK_BOOTTIME barrier; append
checks the original selection-event observation timestamp and bound observer
epoch under the same lock, so an older
observed in-flight transfer cannot repopulate cleared history. Native timestamps
are client observation times, since the protocol has no server event timestamp.
The Clear CLI stops the owned user collector, clears under the lock, and starts
it only if consent is still enabled; this also drops undispatched events. A
service failure clears and revokes consent, and the supervisor independently
polls for revocation every half-second and terminates its own observer, with a
three-second kill fallback.
Restart skips the current
selection without saving a startup snapshot or fingerprint. Capture age is checked before append and retained entries preserve their original
capture age, so delayed or suspended supervision cannot extend retention. The barrier is
runtime-only and resets with the user runtime directory each boot.

`clipboard_native_input.py` uses a disposable compositor, private buses/HOME,
synthetic real Wayland producers and an editable document observer. It exercises
actual launcher pointer/keyboard controls, clipboard reads/writes and explicit
paste on both compositor variants. Only user-service supervision is adapted to
the private fixture; no host clipboard, user service or account is accessed. It
uses the first advertised seat; multi-seat collection is not certified. It
also checks initial empty and same-byte future offers, clear/restart and pending
clear barriers, retired result identities, both marker offers during replacement,
transfer bounds, failed/unsupported provider readiness, a paused supervisor with
an injected BOOTTIME advance, and Disable. The virtual clock case does not
suspend the machine. Backend tests cover storage validation separately.

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

## Confirmed application termination

Force Quit is a separate confirmation dialog; opening an application context
menu never terminates it. Preparation resolves each exact selected window in the
owned compositor session. Ambiguous titles, legacy client-reported process IDs,
the desktop shell, and processes owning unselected windows are rejected.
Confirmations are private runtime receipts, expire after 60 seconds, and are
consumed before dispatch. Cancellation removes a receipt without signalling.

Confirmation rechecks the compositor instance and window/container/process
association, then opens Linux pidfds and rechecks each process start time and
executable identity before any signal. It sends SIGKILL through those handles,
never through broad application names or a reusable PID alone. All selected
identities must validate before any signal is sent. A changed/exited process
requires a fresh selection. The response reports termination requests rather
than asserting that a process has already finished exiting. The UI serializes
requests and binds replies to selection generations; a delayed result for an
older app cannot populate another app's confirmation.

`appearance.dockFiles` contains file references only. Context removal edits the
reference list, never the resource. Opening uses a validated, readable regular
file path and the configured GIO default application outside the shell service
cgroup; unavailable targets display an error. These explicit path references do
not claim filesystem-wide tracking of a target moved elsewhere.

### Scoped introspection runtime corrections

Desktop Python runtimes deduplicate repeated deprecated-attribute registration in
PyGObject while retaining the latest replacement target. Importing current GLib
and GioUnix APIs is quiet; deliberately accessing a legacy alias still diagnoses
its deprecation. Python extension compilation includes Python.h before system
headers so Python's feature-test macros initialize before the newer C library's
defaults. The dbus-python repeated-import test follows the same header ordering.

PyGObject's bundled introspection subproject also defines a standalone WarnLib
scanner fixture with deliberately malformed declarations. Binding tests do not use
that namespace. Its library and GIR targets remain explicitly buildable but are
not default build artifacts; no tests or warning diagnostics are disabled. A build
guard asserts that only those target defaults change, required binding typelibs
remain dependencies, no binding test begins using WarnLib and upstream tests stay
enabled. Dependency changes fail this guard for review rather than silently
omitting a newly required fixture.

## Authorization dialog integration boundary

`integrations.authorization.enable` opts into the packaged session authorization
agent. It defaults to false. The integration adds a Sway session startup entry;
it does not change authorization rules, authentication methods or privileged
host access. Consumers retain that policy. Existing sessions are not interrupted
when the declaration is rebuilt or the compositor configuration is reloaded.

The GTK appearance bridge is loaded only by the agent wrapper and follows live
interface theme and color-scheme settings. Password visibility controls are
provided by the existing agent patch. `authorization-agent` exposes the package;
`checks.x86_64-linux.authorization-appearance` compiles the synthetic GTK fixture
with warnings treated as errors and checks light/dark/default propagation under
a private virtual display, without authenticating or handling credentials.

### Accessibility application lifetime and protocol verification

The command launcher temporarily constructs a core application before the GUI
application. Destroying that temporary application clears Qt accessibility factory
registrations. The scoped shell-runtime patch restores the Quick module after the
GUI application's lifetime begins; otherwise a visible window registers its app
but exposes no accessible children. This affects both ordinary and shell-owned
windows, so the corrected runtime is used by the shell, authentication surfaces,
IPC tools and test environment.

`tests/desktop/accessibility_protocol.py` deliberately enables accessibility in a
private compositor, session bus and runtime created before service activation. It
launches its own accessibility bus and registry, validates their private address
and enabled/screen-reader status, registers an actual protocol listener and checks
names, roles, focus, keyboard activation and state-change signals. Full-shell mode
also inspects editable Settings search and the Files view selector. The fixture
asserts that the live accessibility socket is unchanged. This is protocol-level
verification; audible screen-reader output and physical assistive devices still
require their own acceptance evidence.

### Network locations

Files exposes Network and Control+K for explicit server connections. The optional
`integrations.networkLocations.enable` setting supplies the host's GVfs provider
package to the helper; it defaults to false and does not enable host services.
The consumer must configure its GVfs service, network access and keyring policy.
Missing protocol providers are reported, rather than simulated.

The JSON-lines helper uses Gio asynchronous mount, enumeration and unmount APIs.
Server addresses cannot contain credentials, queries or fragments. Authentication
and keyring choices belong to Gtk.MountOperation; credentials never cross the
shell interface or enter connection settings. Connections are session-only.
Cancellation dismisses toolkit dialogs and rejects late callbacks. Mount inventory
remains authoritative: cancellation does not promise rollback of a provider that
has already completed a mount. Provider error text is mapped to fixed categories.

The network pane reads at most 100 entries, scans at most 1000 provider records,
and has an eight-second browse deadline. Connect and disconnect have cancellable
120-second and 30-second deadlines. It supports read-only remote folder navigation;
full Files operations are offered only when the provider returns a real local
filesystem path. Files operations through FUSE retain the ordinary Files backend's
limitations; this is not a guarantee of offline support or bounded kernel I/O.

`test_network_locations.py` checks provider failures, credential rejection,
redaction, cancellation, stale callbacks and bounded listing with fake providers.
`test_network_locations_qml.py` runs the actual Qt state/pane and child-process
protocol. `network_locations_provider.py --provider GVFS_PACKAGE` uses an anonymous
loopback DAV fixture and a private D-Bus/GVfs session, with FUSE disabled, to verify
real connect, browse and disconnect without user accounts or existing mounts.
Other protocols and authenticated server/keyring interactions still need their
own provider acceptance tests.

Desktop placement policies are user session data in
`$XDG_STATE_HOME/lumen-desktop/spaces.json`, updated under a cross-process lock
with atomic replacement. `lumen-spaces watch` applies assignments to newly mapped
windows, including delayed advertised application identity, without a focus
command. Exact identities distinguish Wayland app IDs from X11 classes; these
are advertised identifiers and do not authenticate an application. Assignments
apply to subsequent windows, not all existing windows retroactively.

Logical desktop UUIDs survive compositor restarts by name, and preserve identity
through renames observed in the same compositor session. Renaming a desktop while
the policy service is stopped cannot be inferred. Recent-use order is a separate
list; names, compositor IDs and window membership are never renumbered. The
Overview switch controls automatic ordering and Control Left/Right follows that
published order on the focused output, stopping at either end. Empty assigned
desktops can be recreated on their recorded connected output. Explicit desktop
deletion/forgetting is not currently exposed.

A display assignment chooses its currently visible desktop. Hardware identity
uses make/model/serial when available and a clearly labelled connector fallback
otherwise. Disconnected targets retain their assignment and leave new windows
where the compositor placed them; reconnection resumes matching. Overview shows
unavailable saved assignments. The all-desktop assignment is not implemented by
the native compositor: the backend rejects it rather than substituting floating
sticky windows. This remains an implementation gap.

### Desktop arrangements

The opt-in desktop file surface supports individual icon dragging and expandable
stacks grouped by file kind, modification-date bucket or colour tags. Folders
remain individual items. Context menus switch grouping and reset arrangement;
stack context menus clear hidden file selections before exposing file actions.
Freeform positions apply when grouping is disabled and survive grouping changes.
Positions are normalized to the usable desktop bounds, so display-size changes
keep placed items reachable. Items that do not fit the automatic grid are counted
with an explicit message and remain available through Open Desktop in Files.

`desktop_layout.py` stores private, atomic metadata per directory identity. It
never moves, renames or writes the desktop files themselves. Item inode identity
preserves ordinary rename placement; hard links receive distinct name suffixes.
Authoritative observed removals prune obsolete placements. This does not promise
tracking an inode that is removed and reused between two directory snapshots.
Stack expansion is temporary; grouping and freeform positions persist.

The layer input mask is the union of displayed item rectangles. Empty desktop
areas continue to the surface below. A visible modal menu or confirmation owns
its usable desktop region while open. `desktop_files_render.py --layout` verifies
real pointer dragging, persisted metadata, rename identity, stack expansion and
empty-area click-through in a disposable nested compositor, alongside existing
selection and destructive-confirmation checks. Multi-item dragging uses one frozen selection snapshot and one clamped delta,
preserving relative offsets at screen edges. Whole-stack anchors also persist;
expanded children follow the anchor and wrap into neighbouring columns as needed.
`desktop_files_render.py --layout --multi-drag` verifies actual multi-item and
expanded-stack pointer gestures, grouping-switch persistence and file preservation.
Cross-application native file drops are not implemented by this slice.

Hot Corners use native compositor transitions rather than pointer polling or
input-capturing overlay windows. The `lumen-corner` tick protocol version1 carries
only corner/output identity, the modifier snapshot, and lock/fullscreen/adjacent
inhibitions. Entry is within one logical pixel; native observation retains a
corner latch until leaving an eight-pixel region. Modifier presses after entry
do not trigger an action. Backend policy additionally latches suppressed entries,
fails closed on incomplete inhibition information, and never activates while
locked. Fullscreen and adjacent-display corners require explicit opt-in.

Settings > Desktop & Dock offers four independent configurations, all disabled
by default, saved atomically in the user's `hot-corners.json` state file. Actions
are a fixed enum: overview, application windows, desktop, notifications,
applications, lock screen, or display power off. Display power off does not
suspend the machine. No arbitrary command execution is accepted. Screensaver and
notes actions have no implemented provider and are not offered. The watcher
reloads validated configuration on entry without restarting the compositor.

## Native paired placement observations

A compatible compositor supplies `lumen_pair` facts. The observer validates exact
member and partner IDs, opposite sides, a finite ratio, and reciprocal two-window
membership in the same desktop/output before reporting `placement: paired` and
its title-free `pair` object. A full-screen survivor is reported independently.
Absent compositor facts remain unknown. These fields do not create a command
broker or promote advertised application IDs into verified application identity.

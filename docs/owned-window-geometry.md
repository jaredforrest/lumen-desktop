# Opt-in owned-window placement

`windows.rememberOwnedGeometry` defaults to `false`. Its Settings control is
“Remember Settings and Calendar window placement”. The only registrations are
`settings.main` and `calendar.main`, declared by those singleton main components.
Files has no durable window-slot identity and is deliberately unsupported. No
window title, process ID, document path, session index or foreign application is
used as a durable registration key.

The existing `WindowGeometry` observer serializes fitting, initial restoration
and placement capture. Live actions still capture and revalidate the actual
parent process/start, compositor socket, native container and native relationship
facts. No saved record is a native action capability. Transient/modal, unknown
relationship, tiled, paired, fullscreen, filled, zoomed, minimized and hidden
placements are excluded. Existing preferred-size registration and workarea
margin policies remain in force.

## Capture and reopening

The compositor advertises `owned_geometry_events` and emits the window change
`lumen_geometry` once when an interactive floating move or resize finishes with
changed geometry. Pointer moves/resizes and the existing tablet move completion
use their existing gesture lifecycle; cancellation/destruction does not add a
completion event. This is not a per-frame stream. Programmatic IPC or
client-authored geometry changes, and keyboard resize commands, are not claimed
as captured gestures.

The helper samples the exact current native target after completion, rechecks
ordinary placement and the published opt-in, and debounces a stable capture for
200 ms using the suspend-aware clock. Only successful atomic writes count as a
remembered placement. Closing/crashing before capture acknowledgement can retain
the previous record. No session-saving or crash-proof last-frame claim is made.

A newly mapped registered main window restores the last successful client size
and output-relative outer position once, then follows ordinary fitting. Hiding
and publicly reopening the same component may create a new native mapping; this
also restores once. Ordinary fits/workspace updates do not replay saved geometry.
Observer epochs reject retired responses, including stale errors and ready/fit
acknowledgements.

A still-connected saved connector may be selected using its existing active
workspace. A missing connector leaves the new window on an existing reachable
workspace. Its position and dimensions are clamped to that current workarea and
minimum size; existing margins remain bounded by the minimum size. No workspace
identity is saved or resurrected. Output loss/resizing and helper-authored clamps
do not overwrite the preferred record. A subsequent completed ordinary gesture
can establish a new record. Connector names outside the supported command-safe
syntax fall back to the current workspace.

## State and failure contract

Persist and back up the **directory**
`$XDG_STATE_HOME/lumen-desktop/owned-geometry/` (default
`~/.local/state/lumen-desktop/owned-geometry/`). It contains `placements.json` and
`.placements.lock`; do not bind-mount an individual atomically replaced file.
The directory is private mode 0700; files/lock are mode 0600. JSON version 1 has
at most the two allowlisted owner records: connector name, relative integer x/y
and client width/height. It contains no process/container/workspace IDs, window
contents or paths. The file is bounded to 16 KiB and dimensions to 32768.

Writers serialize with a nonblocking lock and a 500 ms suspend-aware contention
budget. Replacement uses a private temporary file, fsync and rename. Unsafe
permissions, nonregular files, symlinks, malformed/oversized state, lock contention
or publication errors refuse the affected operation and preserve previous data.
This does not promise a generic deadline for uninterruptible filesystem I/O.
Disablement stops restores/captures; prior records remain inert rather than being
erased. Public code declares this data contract; consumer configuration owns
persistence, backup and retention policy.

An old compositor continues normal fitting with the preference off. With it on,
the owned status message visibly reports that an updated compositor at next
login is required; there is no guessed capture, polling fallback or false
availability claim. Storage/restoration errors use the same accessible status
surface without replacing a successful ordinary fit.

## Acceptance

The Rust 2024 `desktop-policy-rust` fixture runs `owned-geometry` and
`owned-geometry-legacy` against private two-output compositors. It checks real
pointer-committed records for both registered keys, process restart, stale
container refusal, output shrink and removal, unsupported registration, and
the old compositor's explicit capability refusal. Its `owned-components` and
`owned-components-legacy` scenarios load the production Settings and Calendar
components and check fitting, capability status, and the placement status
footer. These fixtures use private runtime, state and compositor endpoints.

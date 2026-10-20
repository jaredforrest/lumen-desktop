# Files navigation history

Each successful runtime history entry owns its location, view, filter, selection,
current row, selection anchor and viewports. Independent tabs copy their entry
stacks. Back and Forward restore the captured entry; a successful new route
truncates the forward branch. Failed or superseded lists do not commit a route.
Refresh and reconnect reconcile the committed entry against the returned rows.
An unavailable tab destination shows no rows from another tab and keeps its
stored stack available for navigation.

Selection uses the backend's advertised device/inode identity plus path. A
matching path with a different known identity is not selected; a uniquely
identified renamed row may be reconciled, while ambiguous alternate hard links
are not guessed. Unknown identities cannot replace a previously known identity.
These are presentation annotations, not filesystem mutation authorization.

List, grid and gallery retain bounded offsets and row anchors; anchors reconcile
against new sort coordinates and offsets clamp to the actual viewport. Columns
retain horizontal position and the separate ancestor-list viewports, applying
restoration after asynchronous column results arrive. Keyboard movement also
keeps the current column row visible.

History is limited to 128 entries per tab. A snapshot supports up to 512 selected
rows and 65,536 UTF-16 code units of combined selection paths. Overflow explicitly clears
the restored selection and current row and displays a notice; it never silently
restores a subset. Queries are limited to 4096 characters, paths to 4096 characters,
column lists to three, and finite offsets to 10 million pixels. Typical snapshots
are much smaller; the selection path bound limits retained selection strings to
8,388,608 UTF-16 code units per tab across the maximum entry count, plus bounded metadata.
History is runtime-only and does not add a directory preference database.
Restoration suppresses global preference writes; explicit user view changes
continue to use the existing global preference policy.

`test_files_history_model.cjs` covers immutable transaction and tab behavior,
identity reconciliation, bounds and overflow. `files_history_input.py` copies
assets from an immutable installed shell into a private compositor with synthetic
directories. It exercises actual pointer selection, physical shortcuts, all four
scrolled views, filters, sort changes, independent tabs, failed destinations,
real backend termination/reconnect, paused-backend response races and replaced
paths. Both compositor variants run in CI. No personal files or clipboard are
used. The service process is the real Files backend; only read-only fixture state
and explicit navigation setup are exposed over the private test IPC.

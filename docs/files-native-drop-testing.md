# Native Files drag acceptance

Run `nix develop --command nu tests/ci/files-native-drop.nu` from a clean checkout.
The script builds immutable providers, resolves their store paths before entering
the private application namespace, and runs basic/effects compositor cases
sequentially with independent Qt and GTK receiver processes. It uses the persistent
`test-pointer` provider, not the one-shot `test-drag-pointer` provider.

The fixture checks list, grid, columns and gallery offers, a selected group,
encoded spaces, `#` and Unicode in URI payloads, and actual receiver file reads.
Qt's displayed URL strings are retained separately from raw `text/uri-list`
bytes. Incoming cases cover Copy, offers without Copy, Escape, non-URI MIME,
relative/malformed/nonlocal URI inputs, conflict skipping, folder targets and
spring navigation. Real file-job records, destination contents and retained
source bytes distinguish native rejection from a later file-service error.
The original namespace and every owned process are removed after each case;
logs, native window geometry, screenshots and JSON receipts remain under
`artifacts/files-native-drop-*`.

GTK3.24.52's `gdk/wayland/gdkdnd-wayland.c`, function `gdk_to_wl_actions`, maps
`GDK_ACTION_LINK` to the Wayland Copy action. The GTK case records this intent
and the selected Copy action; it does not claim a native Link-only rejection.
The Qt case separately exercises an offer without Copy for that source intent.
These fixtures do not establish compatibility with arbitrary applications,
other operating systems, arbitrary MIME data or native Move/Link operations.

Eligible native Copy jobs retain recoverable Undo history. Undo moves an
unchanged created output into standard Trash; Put Back restores it. The drag
matrix checks that this history is available; the separate Copy Undo tests
exercise its identity checks, recovery outcomes and restore route. Historical
drag receipts made before Copy Undo was introduced retain their original
provider scope and do not establish the newer Undo behavior.

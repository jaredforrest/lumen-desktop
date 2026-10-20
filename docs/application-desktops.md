# Application desktop assignment

Window Overview offers No assignment, All desktops on current display, a logical
desktop, or a connected display. The advertised application identity is exact
(`wayland:app_id` or `x11:class`), persisted in the existing private state file;
it is a placement hint, not an authenticated application identity.

The all-desktop policy uses native sticky membership for independent floating
windows. Each window follows the visible desktop on its current display.
Multiple instances keep their own display; no windows are copied to other
physical displays. A user-requested display move and compositor disconnect
fallback preserve this membership on the resulting display. Selecting a desktop
or display assignment replaces the all-desktop policy while retaining that
assignment's existing future-window placement semantics.

Support is intentionally narrower than a promise for every window. Tiled,
fullscreen, paired, minimized, hidden, owned/modal and native-grouped windows
retain their layout and saved placements. Unknown ancestry (including the
currently unsupported X11 ancestry provider) remains pending. Overview reports
the specific reason for the selected window. Normal native windows are eligible
only when the compositor reports supported ancestry with no parent/modal role.
No title-based role inference, forced floating, automatic ungrouping or saved
placement rewriting is performed. Existing grouped application instances reject
the assignment before saving; turn off window groups first. If a future instance
is unsupported, the persisted application policy stays pending for that window.

The watcher subscribes with an acknowledged native IPC handshake before its
initial snapshot. It reconciles current eligible instances after restart, new
windows, layout/output events, and pending role changes that do not emit events.
A one-second fallback checks only active membership policies. Assignment changes
through Overview and the existing command adapter also reconcile immediately.

Only sticky changes made by this policy are reversed when clearing/replacing an
assignment or when a managed window becomes ineligible. Ownership records bind
the compositor socket session, container ID, PID and process start time. Existing
sticky windows remain sticky; reused native IDs or process instances never
inherit an old ownership record. Native IPC does not expose the author of a
concurrent sticky change; the policy owns the property it enabled while the
assignment remains active. It does not claim to arbitrate simultaneous external
writers. Closing/restarting the compositor drops old ownership without sending
commands to replacement windows.

## Verification

```sh
nix develop --command desktop-test-python tests/desktop/test_application_desktops.py
nix develop --command desktop-test-python tests/desktop/test_spaces.py
nix build .#shell .#compositor-basic .#compositor .#test-window-relationships
```

Run `tests/desktop/application_desktops_native.py` with `--launcher` pointing to
the built shell launcher, `--sway` to the basic or effects compositor, `--client`
to the built `lumen-test-window-relationships`, and a new `--output` directory.
Add `--effects` for the effects compositor. CI runs both and retains snapshots,
Overview captures and the result manifest. The fixture uses private HOME/XDG,
activation-free D-Bus, synthetic application processes and nested outputs. It
uses the production Overview assignment control with real keyboard input and
never contacts a host compositor or personal application.

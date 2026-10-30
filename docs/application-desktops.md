# Application desktop assignment

Window Overview offers No assignment, All desktops on current display, a logical
desktop, or a connected display. The advertised application identity is exact
(`wayland:app_id` or `x11:class`), persisted in the existing private state file;
it is a placement hint, not an authenticated application identity.

The supported native provider presents ordinary tiled, floating and grouped
windows on the visible desktop of their owning display. The original container,
foreign identity, workspace, parent, sibling order, tiling ratios and saved
placement remain structural origin facts. A tiled window uses a floating
presentation in the existing floating scene layer; this deliberately does not
promise reference geometry or animation. It shares ordinary floating stacking,
raising and hit testing rather than using a permanent topmost overlay. No scene
or native window is cloned, floated structurally or automatically ungrouped.

Group selection and one-at-a-time presentation, application hide, minimize,
Show Desktop and native session-lock shielding remain independent. Projection
overrides only workspace visibility. A foreground fullscreen request deliberately
activates the retained origin desktop, then enters normal fullscreen. Background
or unsupported requests cannot move the window or silently activate its origin.
Saved fullscreen restoration remains an internal placement operation; restoring
without focus preserves the current desktop and focused application.

Each instance keeps its owning display. Explicit user moves and output fallback
update the origin and invalidate stale presentation witnesses. The provider
reports bounded versioned origin/effective metadata and a monotonic revision;
typed changes require the exact foreign identity, PID, origin workspace/output
and revision. Effective placement is separate from structural occupancy and
restoration. Unsupported or suspended metadata remains an inventory member,
while affected scoped actions refuse unavailable placement. Global activation
retains normal native restore behavior.

Older providers and X11 windows retain the independent-floating sticky path.
Their unsupported grouped/tiled, relationship and saved-placement constraints
remain explicit pending states. The public assignment route admits intact
native groups only when valid native projection metadata is present; missing,
malformed or unsupported-version metadata does not waive legacy constraints.
Multiple instances remain on their own displays, and no window is copied onto
a different physical display.

The watcher subscribes with an acknowledged native IPC handshake before its
initial snapshot. It reconciles current eligible instances after restart, new
windows, layout/output events, and pending role changes that do not emit events.
A one-second fallback checks only active membership policies. Assignment changes
through Overview and the existing command adapter also reconcile immediately.

Only native projection or sticky changes owned by this policy are reversed when
clearing or replacing an assignment. A suspended native instance retains its
claim until an exact checked clear becomes possible. Ownership records bind
the compositor socket session, container ID, PID and process start time. Existing
sticky windows remain sticky; reused native IDs or process instances never
inherit an old ownership record. Native IPC does not expose the author of a
concurrent sticky change; the policy owns the property it enabled while the
assignment remains active. It does not claim to arbitrate simultaneous external
writers. Closing/restarting the compositor drops old ownership without sending
commands to replacement windows.

## Verification

```sh
nix-shell -p cargo rustc --run 'CARGO_TARGET_DIR=/tmp/lumen-spaces-target cargo test --offline --manifest-path res/lumen-desktop/spaces/Cargo.toml'
nix build .#shell .#compositor-basic .#compositor .#test-window-relationships
```

Run `lumen-desktop-policy-fixture application` with `--launcher` pointing to
the built shell launcher, `--sway` to the basic or effects compositor, `--client`
to the built `lumen-test-window-relationships`, and a new `--output` directory.
Add `--effects` for the effects compositor. The fixture records the actual
GET_VERSION response and selects a separate capability-aware corpus. An unknown
capability version fails closed. CI runs both renderers with native projection
and both genuinely capability-absent providers built with
`allDesktopProjection = false`; it never fabricates an absent capability on a
modern provider. The legacy nine-check sticky/tiled/group-refusal corpus remains
intact. The seven-check native corpus checks origin/effective separation,
preexisting sticky ownership, tiled/fullscreen eligibility, opposing group
visibility masks, actual parent/modal changes, watcher restart and survival,
future instances and output lifecycle. It asserts native metadata and structure;
the provider's separate acceptance covers scene paint and hit testing.
CI retains snapshots,
Overview captures and the result manifest. The fixture uses private HOME/XDG,
activation-free D-Bus, synthetic application processes and nested outputs. It
uses the production Overview assignment control with real keyboard input and
never contacts a host compositor or personal application.

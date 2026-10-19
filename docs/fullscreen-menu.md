# Fullscreen menu visibility

`menuBar.fullscreenVisibility` is a presentation preference with the closed values `automatic` (the project default) and `always`. It is part of the public desktop settings schema and the existing settings-package contract, not a second machine-policy option in the NixOS module. Consumers can include `menuBar.fullscreenVisibility = "always";` in their settings source. The Menu bar and clock pane edits the same value through the existing typed settings transaction. Older preference snapshots without the field use automatic behavior.

On a desktop, the menu remains visible and reserves its normal work area. An actual fullscreen container causes the menu on that output to use the overlay layer with zero reserved area. Automatic mode reveals it at the top edge, keeps it while its menu is open and hides it after the pointer leaves. The reveal sensor has no keyboard focus. A keyboard-opened menu also remains reachable; Escape closes it without leaving fullscreen. Always mode retains the visible menu. A live preference change does not change the client's fullscreen state.

A single read-only observer obtains visible workspace and fullscreen-container state through compositor IPC. Workspace structural markers do not count as fullscreen apps. Workspace fullscreen affects its current output; global fullscreen affects all active outputs. No active-title inference, window resizing, host policy change or simulated app-owned interface is involved. Disconnecting an output drops its state; surviving outputs follow the migrated container. Observation failure clears cached state and retries, retaining the ordinary menu route rather than a stale overlay.

The 500 ms leave delay and one-pixel trigger are explicit project interaction policy, not measured external reference values. Exact reference animation, timing and geometry remain unverified.

```sh
nix develop --command desktop-test-python tests/desktop/fullscreen_menu_native.py --launcher /absolute/path/to/lumen-desktop-shell --sway /absolute/path/to/sway --pointer-tool /absolute/path/to/wlrctl --output /absolute/new/evidence-directory
```

The fixture uses two private outputs, a real native client and actual pointer/keyboard input. It checks presented pixels, app input, keyboard invocation, menu interaction, live preference updates, workspace/global modes and removal of the fullscreen output. Use `--effects` for the effects compositor; it explicitly requests genuine software GLES. Evidence includes captures, native state and pointer measurements. Unit tests additionally cover inactive workspaces and structural markers; settings tests use the real Nix evaluator and disposable sources.

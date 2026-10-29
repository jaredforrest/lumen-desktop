# Menu visibility

`menuBar.visibility` is the sole typed presentation setting. It is part of the public settings-package contract and the Menu bar and clock pane has one control for it. It does not introduce machine policy in the NixOS module.

| Value | Ordinary desktop | Fullscreen output |
| --- | --- | --- |
| `always` | Visible | Visible |
| `desktop` (default) | Visible | Top-edge reveal |
| `fullscreen` | Top-edge reveal | Visible |
| `auto_hide` | Top-edge reveal | Top-edge reveal |

Consumers can set `menuBar.visibility = "auto_hide";`. A persistently visible desktop bar reserves its normal work area. Autohiding bars and all fullscreen bars use the overlay layer and reserve no work area. Reveal is local to the pointer's output, retains an open menu, and hides after leave. The sensor takes no keyboard focus. Keyboard invocation reveals hidden menus; Escape returns input to the native client without changing fullscreen state. Presentation updates do not resize or change a client's fullscreen mode.

The published `fullscreenVisibility` input is migrated explicitly: `automatic` becomes `visibility = "desktop"`; `always` becomes `visibility = "always"`. This occurs in the Nix evaluator, settings command input and old runtime-snapshot readers. Canonical outputs omit the alias and the next successful settings transaction serializes only the canonical field, preserving unrelated preferences. Inputs containing both fields are rejected, even if they would agree; invalid legacy values are rejected. No second authority or hidden compatibility control remains. Missing values retain the previous default behavior.

A single read-only observer obtains visible workspace and fullscreen-container state through compositor IPC. Workspace structural markers do not count as fullscreen apps. Workspace fullscreen affects its current output; global fullscreen affects all active outputs. No active-title inference, window resizing, host policy change or simulated app-owned interface is involved. Disconnecting an output drops its state; surviving outputs follow the migrated container. Observation failure clears cached state and retries, retaining the ordinary menu route rather than a stale overlay.

The 500 ms leave delay and one-pixel trigger are explicit project interaction policy, not measured external reference values. Exact reference animation, timing and geometry remain unverified.

```sh
nix build .#test-fullscreen-menu-fixture -o artifacts/fullscreen-menu-fixture
nix develop --command nu tests/ci/run-native.nu fullscreen_menu_native -- artifacts/fullscreen-menu-fixture/bin/lumen-fullscreen-menu-fixture --launcher /absolute/path/to/lumen-desktop-shell --sway /absolute/path/to/sway --pointer-tool /absolute/path/to/wlrctl --source . --output /absolute/new/evidence-directory
```

The fixture uses two private outputs, a real native client and actual pointer/keyboard input. It checks old-snapshot startup, all four desktop/fullscreen policies, presented pixels, app input, keyboard invocation, menu interaction, live preference updates, workspace/global modes and removal of the fullscreen output. Use `--effects` for the effects compositor; it explicitly requests genuine software GLES. Evidence includes captures, native state and pointer measurements. Unit tests additionally cover inactive workspaces and structural markers; settings tests use the real Nix evaluator and disposable sources.

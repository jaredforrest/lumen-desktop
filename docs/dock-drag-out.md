# Dock drag-out removal

Dragging a configurable pinned application, file alias, or folder alias outside the Dock hit region and releasing removes only its saved Dock representation. A visible removal cue appears outside the region. The gesture uses the MouseArea implicit pointer grab and its local coordinates, including rotated side shelves; no global coordinate inference or file deletion is used. Existing in-shelf application reordering remains available.

The press captures the exact item and model generation. Model replacement, disappearance, a settings transaction becoming busy, or Escape cancels the whole gesture. Required shelf entries and unconfigured running/recent entries cannot be removed this way. A running application can remain in the running section after unpinning. Unknown saved application IDs remain in preferences.

Settings writes use the existing appearance transaction protocol. No host policy, application process, file, or folder is removed. The native fixture uses the actual Dock, DockModel and SettingsState with a private JSON-backed command transport; it records every requested preference write and restarts the shell to verify persistence. It tests actual pointer crossings and release, Escape, busy/removal/replacement cancellation, protected entries and preservation of original resources. It does not claim exact external-reference animation measurements.

Run either compositor variant after building the shell and test pointer package:

```sh
nix build .#test-dock-interaction-fixture --out-link artifacts/dock-interaction-fixture
nix develop --command ./artifacts/dock-interaction-fixture/bin/lumen-dock-interaction-fixture drag --launcher /path/to/lumen-desktop-shell --sway /path/to/sway --pointer-tool /path/to/wlrctl --source . --output /tmp/dock-drag-out
```

Add `--effects` for the effects compositor; this explicitly requests its supported software rendering mode. CI retains result, pointer-state/write evidence, screenshots, and compositor/QML logs.

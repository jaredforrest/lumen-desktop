# System panel text enlargement

Owned Control Centre tiles, status labels and media labels follow the shared text-size preference. Enlarged Control Centre uses one column; tiles wrap their labels and size their contents naturally. Footer commands and notification widget toggles retain their natural text height. The session confirmation body scrolls and reveals keyboard focus when explanatory or error text exceeds the available window height.

`tests/desktop/system-popover-rust` runs the production shell in a private session with a synthetic read-only calendar, synthetic notification provider and two actual private MPRIS services. It uses a 640 × 480 output at 150% text, a long control label and a long session diagnostic. Now Playing includes supported playback-position and player-volume rows. Actual Tab traversal checks focus bounds against the window and all clipping ancestors; named footer commands must be reached. Captures and text/focus measurements are retained for every surface. No session operation is executed.

Run with the packaged runtime:

```sh
nix build .#test-mpris-mock-fixture --out-link /tmp/lumen-mpris-mock
nix build .#test-system-popover-fixture --out-link /tmp/lumen-system-popover
nix develop --command /tmp/lumen-system-popover/bin/lumen-system-popover-fixture --launcher /absolute/path/to/lumen-desktop-shell --sway /absolute/path/to/sway --mpris-provider /tmp/lumen-mpris-mock/bin/lumen-mpris-mock-fixture --output /absolute/new/evidence-directory
```

Use `--effects --theme light --native-frame on` for the alternate compositor, palette and frame route. The workflow exercises both configurations. This verifies the supported owned surfaces; it does not assert exact external visual measurements or enlargement of third-party applications.

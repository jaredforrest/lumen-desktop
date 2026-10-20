# Retained-browser workflow acceptance

This optional corpus runs an explicitly supplied immutable Brave executable. It tests the retained application directly, with its genuine chooser and supported interfaces. It does not bundle a browser or claim reference-exact appearance.

## Boundary

The shared application runner uses private Linux user, PID, mount, network, IPC and UTS namespaces. It pivots to a disposable root and detaches the original root, then drops capabilities with non-root identity and no-new-privileges. Only immutable dependencies, read-only source, fresh artifacts, synthetic configuration and universal null/random devices are exposed. The application can still establish its own nested sandbox. Unsupported isolation fails closed; the internal entry also rejects ambient execution before creating state.

Every launch has an explicit private `--user-data-dir`. Host profiles, accounts, buses, audio/camera devices and printers are absent. Only a synthetic loopback HTTP server is reachable. Effects runs opt into the shared validated software-graphics device capability; physical rendering and capture devices remain absent. Origin permissions, browser sandbox and download/trust policy stay real. Onboarding and the default-browser startup check are separately disclosed exclusions.

The diagnostic pipe observes browser/page/provider state; it never grants permissions, chooses files or performs UI actions. A persistent full-US Wayland keyboard and native pointer exercise those actions. Exact profile files are read-only witnesses, with history checked after actual native browser exit rather than assuming immediate database flush. Native frame ownership and chooser parent identity are checked separately.

## Reproduce

Start from a committed checkout. Build the pinned dependencies, and supply the installed browser's immutable executable explicitly:

```sh
lumen_browser_launcher=$(nix build .#shell --no-link --print-out-paths)
lumen_browser_compositor=$(nix build .#compositor-basic --no-link --print-out-paths)
lumen_browser_pointer=$(nix build .#test-pointer --no-link --print-out-paths)
lumen_browser_keyboard=$(nix build .#test-physical-keyboard --no-link --print-out-paths)
lumen_browser_accessibility=$(nix build .#test-screen-reader-atspi --no-link --print-out-paths)
lumen_browser_software=$(nix build .#test-application-software-driver --no-link --print-out-paths)
lumen_browser_font=$(nix build .#test-application-font --no-link --print-out-paths)

bash tests/ci/run-native.sh browser_workflows_input \
  nix develop --command desktop-test-python tests/desktop/browser_workflows_input.py \
  --browser /nix/store/EXPLICIT-BROWSER/bin/brave \
  --launcher "$lumen_browser_launcher/bin/lumen-desktop-shell" \
  --sway "$lumen_browser_compositor/bin/sway" \
  --pointer-tool "$lumen_browser_pointer/bin/wlrctl" \
  --keyboard-tool "$lumen_browser_keyboard/bin/lumen-test-physical-keyboard" \
  --font-directory "$lumen_browser_font/share/fonts/truetype" \
  --at-spi-prefix "$lumen_browser_accessibility" \
  --software-driver "$lumen_browser_software" \
  --workflow basic --output artifacts/browser-basic
```

Use a new output directory for every run. Repeat accepted cases with `--workflow downloads`, `library` or `new-save`. `permissions` and `page-fullscreen` retain diagnostic scenarios independently. For effects, build `.#compositor` instead and add `--effects`. The runner retains source hashes/revision, exact dependency paths, version, actual painted captures, native trees, typed per-action observations and provider/filesystem outcomes. Download rows and the private-page sentinel must match an actual browser readback in the presented native screenshot, including foreground pixels; logical provider readiness alone cannot pass a blank frame. Matching permits at most one raster pixel of alignment and three channel values of quantization. The application corpus currently uses one 1400×1000 output at scale one. A passing case never closes an entire multi-action row.

## Accepted scope

| Workflow | Actual exercised behavior |
| --- | --- |
| `basic` | Main/chooser frame ownership; genuine Save As Cancel; new/close/reopen captured tabs; page zoom/reset; browser fullscreen and input return |
| `downloads` | Genuine target chooser; independent running partial, pause/resume same ID, complete with exact whole-file hash, cancel a different transfer and remove its partial |
| `library` | Bookmark add/edit same ID/remove; native private window with independent cookies/local storage; private-history exclusion in live provider and durable history after actual native Exit |
| `new-save` | Genuine chooser accepts a new exact target; filesystem content/hash and actual provider complete state agree |

Overwrite cancel/commit, read-only errors and an actual disconnected mount remain unclosed. The prior overwrite diagnostic is not an accepted scenario. Further tab actions, navigation/draft/error behavior, bookmark folders/import/export/history editing, other permission capabilities, separate profiles/guest/extensions, reader/print/PDF/find, app mode/picture-in-picture and local external-handler consent also remain independently tracked. No unsupported action is silently treated as tested, and no external account/store/device is probed.

## Automatic and namespace checks

`nix flake check` runs the pure invocation, inventory, physical-map, read-only SQLite, actual loopback server, subprocess pipe and ambient-entry rejection regressions. Namespace syscalls are a separate native prerequisite: run `tests/desktop/application_isolation_native.py --output <fresh-directory>` through the pinned development runtime and native warning wrapper. The pure `test_browser_isolation.py` only checks safe preflight and must remain usable in restricted build sandboxes. Optional application runs require the explicit browser dependency and are not inferred from passing unit or namespace checks.

Page-requested fullscreen remains unaccepted: a painted-frame and protocol trace found new window geometry committed against the old buffer, followed by resized buffers without another geometry request. The declared effective bounds therefore remain smaller than the output. The discrepancy is present before diagnostic readback. Browser fullscreen uses the correct painted bounds. Logical fullscreen flags alone cannot close the page-requested action; changing compositor geometry to conceal this sequence is not an accepted repair.

One notification-lifecycle replay lost the application accessibility root after Allow (an actual NoReply response). Separate subsequent basic and effects diagnostics completed all seven checks, including native revocation, with captured bus-owner continuity and unchanged deadlines. These observations are separate from the accepted four-workflow tranche and do not establish a repair for intermittent provider availability. Permission-provider state alone cannot prove a native revoke action.

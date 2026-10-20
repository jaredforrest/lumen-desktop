# Owned typography and constrained windows

`Theme.typography` declares pixel bases: `caption=10`, `metadata=11`, `label=12`, `body=13`, `subheading=15`, `title=16`, `heading=18` and `display=29`. `Theme.textRole(name)` rounds the chosen base at the existing supported text scales 1, 1.15, 1.3 and 1.5. Unknown roles use body. Default sizes remain compatible; fixed-size symbols such as the visibility checkmark and link badge retain their own symbol bounds.

Files, its columns and Calendar use these roles for ordinary text. Files grows text-bearing rows and icon captions, keeps breadcrumbs to one line and omits optional list metadata columns in narrow content. Calendar moves view controls to a separate row when needed, scrolls readable content instead of compressing date cells, derives year cells and timed-hour pitch from actual font metrics, and reveals focused dates and sidebar commands. Event titles may elide within their time allocation; the full accessible name/time and detail action remain available. These are engineering readability constraints, not measured reference geometry.

The actual QML corpus renders 128 cases: both windows, all four presentations, 480×320 and 1060×720, light/dark, and four supported text scales. It requires the actual Qt-resolved default family, correct text bindings, nonblank frames, year line bounds, date-mark contrast, timed-event line bounds and focused date/sidebar reachability. Native acceptance separately uses physical keys for search/filter, date navigation and Escape cancellation at 150% in small windows on both supported compositor variants. No account, volume discovery, host font directory or personal document is used.

```sh
nix build .#shell --out-link /tmp/lumen-type-shell
nix build .#test-owned-font-config --out-link /tmp/lumen-type-fonts
bash tests/ci/run-native.sh owned_typography_qml nix develop --command desktop-test-python tests/desktop/owned_typography_qml.py --launcher /tmp/lumen-type-shell/bin/lumen-desktop-shell --font-config /tmp/lumen-type-fonts --output /tmp/lumen-type-frames
```

Fresh output directories and clean frozen source are required. The namespace wrapper records hashes/revision and rejects source changes during acceptance. `--font-config` must resolve to an immutable font configuration; optional `--font-family` selects the requested family and `--expected-resolved-family` verifies an intentional consumer alias (defaults to the requested family). The public font configuration includes the bundled default family and declared fallback; personalized font policy remains consumer-owned and requires its own acceptance. The software offscreen corpus does not claim physical input or native-mask coverage. Native fixture arguments and retained artifacts are wired into the main check workflow.

# Repeatable window visual evidence

The installed-shell Rust fixture has a focused `--window-visuals` mode. It reuses
the existing private session, compositor, application-profile isolation, keyboard
and screenshot machinery instead of running the complete presentation sequence.
An explicitly supplied immutable external application runs signed out in a new
profile. The native green-button route opens the production layout menu. A
persistent virtual pointer moves over Fill and Top Left; Escape dismisses the
menu without choosing a layout or closing a personal application.

Build the fixture, shell and effects compositor with ordinary Nix commands:

```sh
nix build .#test-installed-smoke-fixture --out-link /tmp/lumen-visual-fixture
nix build .#shell --out-link /tmp/lumen-visual-shell
nix build .#compositor --out-link /tmp/lumen-visual-compositor
```

In Nushell, select the consumer-installed application binary and a fresh output
directory, then run inside the development environment:

```nu
$env.LUMEN_REVIEW_CODEX_BIN = '/nix/store/CONSUMER-PACKAGE/bin/codex-desktop'
$env.LUMEN_RELEASE_SCALE = '1'
nix develop --command /tmp/lumen-visual-fixture/bin/lumen-installed-smoke-fixture --window-visuals --launcher /tmp/lumen-visual-shell/bin/lumen-desktop-shell --sway /tmp/lumen-visual-compositor/bin/sway --effects --theme dark --output /ABSOLUTE/FRESH/OUTPUT
```

The binary path is consumer-specific and must name a real Nix store file.
Use the machine's native resource-slot wrapper when another native fixture is
running. Supported scales are `1`, `1.25` and `1.5`; the physical output stays
2560×1440. Optional `--assets` and `LUMEN_RELEASE_WALLPAPER` retain the existing
consumer asset and wallpaper interfaces. No provider or host setting is changed.

Artifacts include `window-corners.png`, cursor-bearing
`window-menu-hover-1.png` and `window-menu-hover-6.png`, native window geometry,
and `window-visuals.json` with pointer positions, menu geometry and selected row.
The read-only `desktop.windowLayoutStatus` IPC method supplies production bounds;
the fixture does not set selection or substitute a fake menu.

Exit success means evidence collection completed, not that either visual defect
is fixed. Inspect `highlightFollowsPointer` and the original pixels. On the
initial dark baseline, both pointer samples reproduced the selection staying on
row zero. Corner quality remains a visual check: the fixture does not pretend
that a screenshot's existence establishes correct antialiasing or GPU parity.

Live issue captures belong to the consumer's private collection. Do not commit
personal application contents, proprietary artwork or authenticated profiles.

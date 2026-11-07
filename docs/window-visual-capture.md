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

Without verification flags, exit success means evidence collection completed,
not that either visual defect is fixed. Inspect `highlightFollowsPointer` and the original pixels. On the
initial dark baseline, both pointer samples reproduced the selection staying on
row zero. Corner quality remains a visual check: the fixture does not pretend
that a screenshot's existence establishes correct antialiasing or GPU parity.

Live issue captures belong to the consumer's private collection. Do not commit
personal application contents, proprietary artwork or authenticated profiles.

## Regression and hardware modes

Add `--verify-window-hover` to require the highlight to follow both pointer
samples. The fixture then checks Up/Down after pointer input and presses Return
on Top Left. Its captured application must tile rather than enter fullscreen.
The ordinary evidence-only mode still permits recording an unfixed baseline.

Window specimens span most of the output width so the right-hand curve is
tested beyond 2048 physical pixels. Software rendering remains the default.
`--hardware-renderer` is accepted only with `--window-visuals --effects`; select
the consumer's EGL vendor after entering the development shell when needed.
Keep `sway.log`, whose EGL/GL renderer lines identify the actual device. A
hardware request alone is not proof that the driver used acceleration.

At 100% scale, `--corner-reference /path/to/matched/software-capture` compares
both 12×12 title-bar corner regions with that capture's original PNG and native
window geometry. It retains per-corner mean channel errors and requires each to
stay within two byte values. Match wallpaper, application and styling between
the two runs. This is a renderer regression comparison, not a general aesthetic
score. The original wide NVIDIA case exceeds four values on the right corner
while the left stays below one.

The renderer now prefixes its vertex and fragment sources with `#version 100`,
the language version their syntax targets. On the tested NVIDIA GLES context,
the implicit source version left `GL_FRAGMENT_PRECISION_HIGH` undefined despite
23-bit high-float support; an explicit version restores the precision branch.
With matched 2462-pixel-wide specimens, the old right corner's mean byte error
was 4.546 against software and the corrected result was 0.012. The reference
test rejects the old renderer and accepts the corrected renderer. This evidence
is scoped to the tested driver and capture; it does not claim all GPUs are exact.

# Interface text sizing and focus reachability

`appearance.textScale` is a typed presentation preference: `1`, `1.15`, `1.3` or
`1.5`. Settings → Accessibility exposes the same values as 100%, 115%, 130% and 150%.
The normal appearance transaction persists it; unsupported numbers, strings and
booleans are rejected. It does not modify display scale, system fonts or app
configuration. `Theme.textSize(base)` rounds the scaled logical pixel size.

Shared PanelLabel, dialog buttons, control rows, settings toggles, combo boxes and
status messages consume this preference. Wrapped labels drive control height.
Long captions and explanations retain their full text instead of ellipses. Combo
choices and constrained dialog buttons grow vertically. Callers supplying fixed
heights or explicit font sizes still own those layout constraints.

The Settings sidebar wraps page names and grows rows. Focus changes scroll the
selected sidebar or page control into view. Resizing and changing text size also
reveal the focused control. The Appearance accent palette and icon choices wrap
at narrow widths; their cards grow with the content and description.

Acceptance is bounded: shared controls with long translated labels and Settings
Accessibility/Appearance at 480×320 with 150% text are verified. This does not certify
all custom widgets or third-party applications at every scale. Output scaling,
full localisation and touchpad routing remain separate requirements. A later pass
must adapt any app-specific fixed geometry that still clips enlarged shared text.

```nix
appearance.textScale = 1.3;
```

Reproduce:

```sh
nix flake check --print-build-logs --option abort-on-warn true
nix build .#shell --out-link /tmp/lumen-text-shell
nix build .#checks.x86_64-linux.offscreen-qml
nix build .#compositor-basic --out-link /tmp/lumen-text-compositor
nix build .#test-pointer --out-link /tmp/lumen-text-pointer
nix build .#test-text-sizing-fixture --out-link /tmp/lumen-text-fixture
nix develop --command /tmp/lumen-text-fixture/bin/lumen-text-sizing-fixture --launcher /tmp/lumen-text-shell/bin/lumen-desktop-shell --sway /tmp/lumen-text-compositor/bin/sway --pointer-tool /tmp/lumen-text-pointer/bin/wlrctl --output /tmp/lumen-text-evidence
```

The native fixture uses a disposable preference backend, isolated compositor and
virtual keyboard/pointer. It verifies forward/reverse Tab reveal, pointer
activation of revealed controls, sidebar reachability, live text changes and
responsive Appearance options. Captures must contain painted interiors. The
separate Nix evaluator test checks actual typed persistence and rejection.

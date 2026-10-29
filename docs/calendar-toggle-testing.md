# Calendar visibility mark contrast

The calendar visibility controls choose their foreground from the actual opaque tint using the shared semantic contrast helper. The light and dark tint families are preserved.

Run the production QML software-frame corpus:

```sh
nix build .#shell --out-link /tmp/lumen-calendar-shell
nix build .#test-offscreen-font-config --out-link /tmp/lumen-calendar-fonts
nix build .#test-calendar-qa --out-link /tmp/lumen-calendar-qa
nix develop --command nu tests/ci/run-native.nu calendar_toggle_contrast -- /tmp/lumen-calendar-qa/bin/lumen-calendar-qa toggle --assets res/lumen-desktop/shell --launcher /tmp/lumen-calendar-shell/bin/lumen-desktop-shell --font-config /tmp/lumen-calendar-fonts --output /tmp/lumen-calendar-contrast
```

The output directory must be new. The Rust driver uses isolated user, PID, mount and network namespaces with inert bus endpoints and software offscreen Qt; no host display, fonts, accounts or hardware devices are used. The result includes the source hash for the tested CalendarWindow. The offscreen backend reports its unsupported window-mask capability; this test does not claim native mask coverage.

Four cases cover all five actual production tints in light/dark presentation with each contrast preference. Assertions inspect the actual mark binding, require foreground contrast of at least 4.5, and verify that both foreground and fill pixels were painted. Invoking the actual control signal verifies the unchecked mark clears. This is presentation and control-signal coverage, not physical pointer acceptance or reference-exact typography.

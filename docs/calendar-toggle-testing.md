# Calendar visibility mark contrast

The calendar visibility controls choose their foreground from the actual opaque tint using the shared semantic contrast helper. The light and dark tint families are preserved.

Run the actual production QML software-frame corpus from a clean source checkout:

```sh
nix build .#shell --out-link /tmp/lumen-calendar-shell
nix build .#test-offscreen-font-config --out-link /tmp/lumen-calendar-fonts
bash tests/ci/run-native.sh calendar_toggle_contrast nix develop --command desktop-test-python tests/desktop/calendar_toggle_contrast.py --launcher /tmp/lumen-calendar-shell/bin/lumen-desktop-shell --font-config /tmp/lumen-calendar-fonts --output /tmp/lumen-calendar-contrast
```

The output directory must be new. The wrapper records immutable runtime/font hashes, the clean source revision and script hash, and rejects source changes during the run. The fixture uses isolated user, PID, mount and network namespaces, private activation-free buses, and software offscreen Qt; no host display, fonts, accounts or devices are used. The offscreen backend reports its unsupported window-mask capability; this test does not claim native mask coverage.

Four cases cover all five actual production tints in light/dark presentation with each contrast preference. Assertions inspect the actual mark binding, require foreground contrast of at least 4.5, and verify that both foreground and fill pixels were painted. Invoking the actual control signal verifies the unchecked mark clears. This is presentation and control-signal coverage, not physical pointer acceptance or reference-exact typography.

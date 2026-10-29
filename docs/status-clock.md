# Status strip and clock presentation

The `menuBar` settings section belongs to desktop presentation. It does not change
system timezone, time synchronisation, network policy or hardware availability.

```nix
menuBar = {
  order = [ "battery" "sound" "focus" "search" "controls" ];
  hourCycle = "locale"; # or "12", "24"
  locale = ""; # system locale; alternatively e.g. "en_GB"
  showDate = true;
  showWeekday = true;
  showSeconds = false;
};
```

The order is a complete unique permutation. An unavailable item retains its slot
and returns there when available. At constrained width the leading status items
move into the existing overflow menu; the clock remains structurally rightmost.
Battery retains its percentage. Normal clicks open the original controls. Hold
Super while dragging an item to reorder; Escape or vertical release outside the
strip cancels. Focused items support Control Shift Left/Right and Return/Space.
Settings → Desktop & Dock offers accessible move buttons as an alternative to
modifier dragging, plus clock controls. This also provides keyboard editing when
the compositor does not give a layer surface keyboard focus.

`lumen-appearance --set-menu-bar KEY JSON` uses the same atomic typed Nix settings
transaction as other presentation edits. The resolved appearance JSON carries
`menuBarPreferences`; consumers should preserve this section when overriding
presentation defaults. No consumer change is required to retain defaults. Runtime
clock precision changes to seconds only when requested.

Reproduce validation:

```sh
nix flake check --print-build-logs --option abort-on-warn true
nix build .#shell --out-link /tmp/lumen-status-shell
nix build .#checks.x86_64-linux.offscreen-qml
nix build .#compositor-basic --out-link /tmp/lumen-status-compositor
nix build .#test-pointer --out-link /tmp/lumen-status-pointer
nix build .#test-status-clock-fixture --out-link /tmp/lumen-status-fixture
nix develop --command /tmp/lumen-status-fixture/bin/lumen-status-clock-fixture --launcher /tmp/lumen-status-shell/bin/lumen-desktop-shell --sway /tmp/lumen-status-compositor/bin/sway --pointer-tool /tmp/lumen-status-pointer/bin/wlrctl --output /tmp/lumen-status-evidence
```

The native fixture uses an isolated compositor and synthetic availability; it
never sends input to the running desktop. Its capture must contain painted
interiors. Formatting fixtures exercise ordinary digits, localized date text,
12/24-hour midnight, seconds, and both seasonal clock transitions under three
process-local timezones. These tests establish behaviour, not reference pixel
identity. Timezone settings and third-party tray item management are outside this
presentation component.

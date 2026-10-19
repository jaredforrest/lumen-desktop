Desktop validation

Backend suite (Python must provide gi and dbus):
  dbus-run-session -- python3 -m unittest discover -s tests/desktop -p 'test_*.py'
Use a private bus: notification tests must not compete with the installed daemon.

Installed shell smoke (normal desktop account, Python standard library):
  python3 tests/desktop/smoke_installed.py --output /absolute/new/test-directory
Requires installed lumen-desktop-shell, dbus-run-session and bash. It derives pinned
runtime tools/assets from the installed launcher; no hardcoded /nix/store versions.
It starts private headless Sway/D-Bus with its own state/cache/config and test documents,
then checks views, preview, tool-page keyboard navigation, Dock Trash and output scales.
No session restart, display reconfiguration, recording or real file mutation is performed.
Optional --drag-pointer /path/to/test-wlrctl also checks grid/list spring-loaded drops
with a real virtual pointer and no motion after navigation, plus retained source bytes.
Build tests/desktop/drag_pointer.nix with the pinned pkgs argument; it is a test-only
modified wlrctl and MUST NOT be installed or pointed at the user's compositor.
Logs and result.json remain in the requested new directory. Only spawned process groups
are stopped. Never run this as root. This is not physical hardware or screen-reader QA.

Installed read-only health report:
  lumen-desktop-doctor
A nonzero exit indicates at least one failed check. Reports no user document contents,
window titles, environment dump or credentials. Backup/hardware pending notes are explicit.

Appearance and window-control regression checks

Build tests/desktop/runtime.nix with this repository's pinned nixpkgs, then use
its bin/desktop-test-python for tests/desktop/run_backend_tests.py --output /new/dir.
Each backend file gets a separate process and private D-Bus session. The runner
clears production deployment settings and retains only the evaluator dependency.

window_frames_render.py --sway /candidate/bin/sway --pointer-tool /pointer/bin/wlrctl --launcher /candidate-shell/bin/lumen-desktop-shell --effects --output /new/dir
Use --core-only for the native pointer/placement suite without installed GTK/Qt/Electron application fixtures.
checks close, canceled clicks, minimize/restore for tiled siblings and floating
windows across workspaces, reversible fill, disabled utility actions, scale 2,
rounded corner pixels, shadow pixels and GTK/Qt/Electron window loading.

smoke_installed.py additionally accepts --sway /candidate/bin/sway --effects
and --assets /candidate/res/lumen-desktop/shell for a not-yet-installed shell.
It saves Files, Settings, Control Centre and Search screenshots.

appearance_cases.json records all 94 audit findings and their acceptance cases.
Visual-acceptance entries are pending until reviewed; merely listing a case is
not a passing test. App-interior redesigns and unsupported capture/menu protocols
must remain explicitly open rather than being marked fixed by frame tests.

Calendar and Notification Centre regression:
  desktop-test-python tests/desktop/calendar_popover_regression.py --output /tmp/calendar-popover-check

The real QML uses a fixture calendar executable which rejects remote mutations,
a private bus, and in-memory notification transport. Checks cover midnight
follow/browse state, leap years/month ends, real keyboard date selection,
accessible today/selection states, provider colours, stack expansion versus
opening, per-app delivery, typed date/time values, scaling and state reload.
smoke_installed.py also opens the real menu-bar popup and Accounts page, checks
an unscaled bounded viewport, and compares pixels outside the popup against a
checkerboard when --effects is enabled. Credential transport and cache colours
are tested with backend fixtures. Portal/AT-SPI warnings in isolated UI logs
are distinct from Nix build warnings. Physical gestures and screen-reader use
require hardware acceptance. Sway supports 3–5-finger swipes, not a 2-finger
edge swipe. Widget arrangement uses the existing persisted session directory.

Shared utility controls and third appearance repair:
  desktop-test-python tests/desktop/appearance_controls_regression.py --output /new/dir

Private headless compositor, synthetic downloads/media players, disabled session
action execution and temporary state. Checks button/combobox keyboard operation,
idle scrollbars, colour/alpha conversion, power confirmation cancellation, help
search, explicit Downloads overflow, media source/seek isolation, and captures
preview/save/inspector/utility/display layouts in light/dark and high DPI.
A proposed acceptance case is still distinct from a passing regression test.

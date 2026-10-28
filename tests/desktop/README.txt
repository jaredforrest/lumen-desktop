Desktop validation

Remaining GI backend suite (Python must provide gi and dbus):
  dbus-run-session -- python3 -m unittest discover -s tests/desktop -p 'test_*.py'
Use a private bus: notification tests must not compete with the installed daemon.

Installed shell smoke (normal desktop account, Rust 2024):
  nix build .#test-installed-smoke-fixture --out-link artifacts/installed-smoke-fixture
  artifacts/installed-smoke-fixture/bin/lumen-installed-smoke-fixture --launcher artifacts/shell-package/bin/lumen-desktop-shell --sway artifacts/compositor/bin/sway --output /absolute/new/test-directory
Requires installed lumen-desktop-shell and dbus-run-session. It derives pinned
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

Native backend checks are Rust 2024 binaries built from the pinned flake. Run
`nix flake check` for package checks and `nu tests/ci/integration-native.nu` for
private compositor and D-Bus acceptance. Each native fixture writes diagnostics
to a fresh output directory under `artifacts`.

artifacts/window-frames-fixture/bin/lumen-window-frames-fixture --sway /candidate/bin/sway --pointer-tool /pointer/bin/wlrctl --launcher /candidate-shell/bin/lumen-desktop-shell --effects --output /new/dir
Use --core-only for the native pointer/placement suite without installed GTK/Qt/Electron application fixtures.
checks close, canceled clicks, minimize/restore for tiled siblings and floating
windows across workspaces, reversible fill, disabled utility actions, scale 2,
rounded corner pixels, shadow pixels and GTK/Qt/Electron window loading.
The optional full application path currently reaches an existing Obsidian
Electron button-color assertion (gray where green is expected) in both the
former Python fixture and the Rust fixture; the CI core suite passes.

The Rust installed-shell fixture additionally accepts --sway /candidate/bin/sway --effects
and --assets /candidate/res/lumen-desktop/shell for a not-yet-installed shell.
It saves Files, Settings, Control Centre and Search screenshots.

appearance_cases.json records all 94 audit findings and their acceptance cases.
Visual-acceptance entries are pending until reviewed; merely listing a case is
not a passing test. App-interior redesigns and unsupported capture/menu protocols
must remain explicitly open rather than being marked fixed by frame tests.

Calendar and Notification Centre regression:
  nix build .#test-calendar-qa --out-link /tmp/lumen-calendar-qa
  nix build .#shell --out-link /tmp/lumen-calendar-shell
  nix develop --command /tmp/lumen-calendar-qa/bin/lumen-calendar-qa card --assets res/lumen-desktop/shell --launcher /tmp/lumen-calendar-shell/bin/lumen-desktop-shell --output /tmp/lumen-calendar-card-check
  nix develop --command /tmp/lumen-calendar-qa/bin/lumen-calendar-qa window --assets res/lumen-desktop/shell --launcher /tmp/lumen-calendar-shell/bin/lumen-desktop-shell --output /tmp/lumen-calendar-window-check
  nix develop --command /tmp/lumen-calendar-qa/bin/lumen-calendar-qa popover --assets res/lumen-desktop/shell --launcher /tmp/lumen-calendar-shell/bin/lumen-desktop-shell --output /tmp/lumen-calendar-popover-check

The real QML uses a fixture calendar executable which rejects remote mutations,
a private bus, and in-memory notification transport. Checks cover midnight
follow/browse state, leap years/month ends, real keyboard date selection,
accessible today/selection states, provider colours, stack expansion versus
opening, per-app delivery, typed date/time values, scaling and state reload.
The Rust installed-shell fixture also opens the real menu-bar popup and Accounts page, checks
an unscaled bounded viewport, and compares pixels outside the popup against a
checkerboard when --effects is enabled. Credential transport and cache colours
are tested with backend fixtures. Portal/AT-SPI warnings in isolated UI logs
are distinct from Nix build warnings. Physical gestures and screen-reader use
require hardware acceptance. Sway supports 3–5-finger swipes, not a 2-finger
edge swipe. Widget arrangement uses the existing persisted session directory.

Shared utility controls and third appearance repair:
  lumen-appearance-controls-fixture --launcher /path/to/lumen-desktop-shell --sway /path/to/sway --output /new/dir

Private headless compositor, synthetic downloads/media players, disabled session
action execution and temporary state. Checks button/combobox keyboard operation,
idle scrollbars, colour/alpha conversion, power confirmation cancellation, help
search, explicit Downloads overflow, media source/seek isolation, and captures
preview/save/inspector/utility/display layouts in light/dark and high DPI.
A proposed acceptance case is still distinct from a passing regression test.

Deliberate accessibility protocol verification (accessibility stays enabled):
  lumen-accessibility-protocol-fixture \
    --output /tmp/desktop-accessibility-evidence \
    --launcher /absolute/path/to/lumen-desktop-shell \
    --quickshell /absolute/path/to/patched/quickshell \
    --at-spi-prefix /absolute/path/to/at-spi2-core --full-shell
This fixture starts its private bus, registry and compositor and checks the live
accessibility socket remains unchanged. Do not reuse a live desktop bus/address.

Greeter and lock rendering/authentication:
  nix build .#test-auth-render-fixture -o artifacts/auth-render-fixture
  artifacts/auth-render-fixture/bin/lumen-auth-render-fixture --source . --launcher /path/to/lumen-desktop-shell --sway /path/to/sway --output /tmp/lumen-auth-evidence
Runs against a private headless compositor, session bus, greetd socket and
disposable PAM configuration. The Rust fixture compares login/lock pixels,
tests password masking, four display sizes, failure/success and crash coverage.

Persistent desktop assignments and recent-use order:
  lumen-spaces-placement-fixture --launcher /path/to/lumen-desktop-shell --sway /path/to/sway --output /tmp/spaces-proof
Uses a private runtime and bus before starting a headless compositor, launches
real windows, restarts the policy daemon, checks display removal/reconnection,
and proves ordering never changes desktop identity. Rust Spaces unit tests
check exact typed app matching, restart persistence and capability rejection.

Hot Corners:
  nix-build -E 'let pkgs = import <nixpkgs> {}; in pkgs.callPackage ./packages/hot-corners.nix {}' --no-out-link
  nix build .#test-hot-corners-fixture -o artifacts/hot-corners-fixture
  artifacts/hot-corners-fixture/bin/lumen-hot-corners-fixture --sway /path/to/compositor/bin/sway --pointer /path/to/pointer --keyboard wtype --lock /path/to/lumen-lock --output /tmp/lumen-hot-corners
Policy tests cover four corners, modifier entry snapshots, inhibition and latch
semantics, atomic persistence and fixed action dispatch. The Qt test exercises
the real editor's configuration roundtrip on a private bus/runtime. Native
pointer transition acceptance is separate and requires the patched compositor;
these tests do not simulate native evidence.

Owned-surface paint matrix (isolated runtime, session bus and fixture data):
  nix build .#test-surface-matrix-fixture -o artifacts/surface-matrix-fixture
  artifacts/surface-matrix-fixture/bin/lumen-surface-matrix-fixture --launcher /path/to/bin/lumen-desktop-shell --sway /path/to/bin/sway --source res/lumen-desktop --theme dark --native-frame off --output /tmp/lumen-surfaces-dark
Repeat for light/dark and native-frame on/off. The native-frame case applies and
verifies actual compositor decorations; setting a theme environment flag alone
is insufficient. The test closes each fixture before the next capture, rejects
blank interiors from measured pixels, and rejects QML binding/image errors.
Screenshots and result.json are acceptance evidence for software rendering,
not a claim of reference-platform visual equivalence or hardware rendering.

Semantic palette and typography acceptance:
  nix build .#test-typography --out-link /tmp/lumen-type-probe
  nix build .#test-typography-corpus --out-link /tmp/lumen-type-corpus-bin
  nix build .#shell --out-link /tmp/lumen-type-shell
  /tmp/lumen-type-corpus-bin/bin/lumen-typography-corpus --launcher /tmp/lumen-type-shell/bin/lumen-desktop-shell --probe /tmp/lumen-type-probe/bin/typography-probe --output /tmp/lumen-type-corpus
The Rust probe runs a Qt Quick QML specimen with the declared distributable
Interlude, DejaVu, FreeFont and Noto Emoji fonts, independent of host font dirs.
It records Qt Quick text metrics, tabular digit advances, painted
light/dark 1x/2x cases, and color emoji pixels confined to the explicit emoji
row. QML does not expose glyph-run fallback identity or
missing-glyph counts, so this check cannot certify those properties. It also
does not certify reference font metrics or private consumer overrides.
  nix build .#checks.x86_64-linux.source-contracts
  nix build .#checks.x86_64-linux.offscreen-qml
  nix build .#test-live-appearance-fixture -o artifacts/live-appearance-fixture
  artifacts/live-appearance-fixture/bin/lumen-live-appearance-fixture --launcher /tmp/lumen-type-shell/bin/lumen-desktop-shell --sway /path/to/compositor/bin/sway --appearance /path/to/lumen-appearance --output /tmp/lumen-live-appearance
Live appearance tests keep six real owned windows mapped through atomic
light/dark/light file changes, verify stable native window ID/geometry and
reject stale palette rectangles from screenshot pixels. No real session input,
files, accounts or system actions are used by these fixtures.
Application lifecycle native protocol proof:
  lumen-lifecycle-native-fixture lifecycle --launcher /path/to/lumen-desktop-shell --sway /path/to/sway --output /absolute/new/test-directory
Uses a private headless compositor/bus/HOME and a synthetic GTK application with
two dirty in-memory documents. Verifies retained zero-window pidfd identity, Dock
and switcher activation, cooperative app-owned cancel/save/discard, and exit cleanup.
It never sends input to the user's compositor or uses a personal document.

Power status
-----------
The battery backend reads the currently owned UPower aggregate device without
activating a missing service. Enabling UPower remains consumer host policy.
Unavailable, absent and malformed snapshots carry no percentage; a ten-second
frontend expiry also clears stalled results. Percentage zero remains a valid
empty battery. The compact menu button opens the supported Control Centre facts.

Reproduce real provider transitions, owner loss during a reply and replacement:
  nix build .#test-power-provider-fixture --out-link /tmp/lumen-power-provider-fixture
  nix develop --command /tmp/lumen-power-provider-fixture/bin/lumen-power-provider-fixture --output /tmp/lumen-power-proof
This creates its own D-Bus daemon. It never contacts a physical power device.
Rust/QML backend tests and the JavaScript model check cover data bounds, low
battery text semantics, accessible descriptions and frontend timeout/removal.
Physical charging/discharging and exact reference geometry remain unverified.

Files tabs/sidebar input regression:
  nix build .#test-files-layout-fixture --out-link /tmp/lumen-files-layout-fixture
  nix develop --command /tmp/lumen-files-layout-fixture/bin/lumen-files-layout-fixture --launcher /path/to/bin/lumen-desktop-shell --sway /path/to/bin/sway --pointer-tool /path/to/persistent-pointer/bin/wlrctl --output /tmp/lumen-files-layout
Uses the test-pointer flake output, disposable content and a private session.
Checks native pointer/keyboard disclosure, drag/keyboard resize, Escape cancel,
stable tab reorder/close/history/selection and narrow window content bounds.
See docs/files-layout.md for control semantics and the document-tab boundary.

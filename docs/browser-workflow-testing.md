# Retained-browser workflow acceptance

This optional corpus runs an explicitly supplied immutable Brave executable. It tests the retained application directly, with its genuine chooser and supported interfaces. It does not bundle a browser or claim reference-exact appearance.

## Boundary

The shared application runner uses private Linux user, PID, mount, network, IPC and UTS namespaces. It pivots to a disposable root and detaches the original root, then drops capabilities with non-root identity and no-new-privileges. Only immutable dependencies, read-only source, fresh artifacts, synthetic configuration and universal null/random devices are exposed. The application can still establish its own nested sandbox. Unsupported isolation fails closed; the internal entry also rejects ambient execution before creating state.

Every launch has an explicit private `--user-data-dir`. Host profiles, accounts, buses, audio/camera devices and printers are absent. Only a synthetic loopback HTTP server is reachable. Effects runs opt into the shared validated software-graphics device capability; physical rendering and capture devices remain absent. Origin permissions, browser sandbox and download/trust policy stay real. Onboarding and the default-browser startup check are separately disclosed exclusions.

The separate `browser-fonts` Rust 2024 fixture replaces the former Python font audit. Inside a disposable `/home/fixture` and `/artifacts` boundary, pass `--browser`, `--profile`, `--output` and `--cases` (a JSON array with explicit `name`, `family`, `text`, `expectedFamilies` and `expectedGlyphCount`). It checks Chromium's actual platform-font selection for every case and saves a painted PNG plus observations. The fixture package's build runs its validation tests; running a particular font policy still requires the corresponding explicit case file and installed font input.

The diagnostic pipe observes browser/page/provider state; it never grants permissions, chooses files or performs UI actions. A persistent full-US Wayland keyboard and native pointer exercise those actions. Exact profile files are read-only witnesses, with history checked after actual native browser exit rather than assuming immediate database flush. Native frame ownership and chooser parent identity are checked separately.

## Reproduce

Start from a committed checkout in the pinned development shell (`nix develop`). Build the pinned dependencies, and supply the installed browser's immutable executable explicitly:

```nu
let launcher = (nix build .#shell --no-link --print-out-paths | str trim)
let compositor = (nix build .#compositor-basic --no-link --print-out-paths | str trim)
let pointer = (nix build .#test-pointer --no-link --print-out-paths | str trim)
let keyboard = (nix build .#test-physical-keyboard --no-link --print-out-paths | str trim)
let accessibility = (nix build .#test-screen-reader-atspi --no-link --print-out-paths | str trim)
let software = (nix build .#test-application-software-driver --no-link --print-out-paths | str trim)
let font = (nix build .#test-application-font --no-link --print-out-paths | str trim)
let fixture = (nix build .#test-browser-workflows-fixture --no-link --print-out-paths | str trim)

^nu tests/ci/run-native.nu browser_workflows_basic -- ($fixture | path join bin lumen-browser-workflows-fixture) \
  --browser /nix/store/EXPLICIT-BROWSER/bin/brave \
  --launcher ($launcher | path join bin lumen-desktop-shell) \
  --sway ($compositor | path join bin sway) \
  --pointer-tool ($pointer | path join bin wlrctl) \
  --keyboard-tool ($keyboard | path join bin lumen-test-physical-keyboard) \
  --font-directory ($font | path join share fonts truetype) \
  --at-spi-prefix $accessibility \
  --software-driver $software \
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

`nix flake check` runs the pure invocation, inventory, physical-map, read-only SQLite, actual loopback server, subprocess pipe and ambient-entry rejection regressions. The Rust application-isolation check covers safe preflight and allocator policy in restricted build sandboxes. Namespace syscalls are a separate native prerequisite: run `lumen-application-isolation probe --source . --output <fresh-directory>` from the pinned development shell through the native warning wrapper. Optional application runs require the explicit browser dependency and are not inferred from passing unit or namespace checks.

Page-requested fullscreen remains unaccepted: a painted-frame and protocol trace found new window geometry committed against the old buffer, followed by resized buffers without another geometry request. The declared effective bounds therefore remain smaller than the output. The discrepancy is present before diagnostic readback. Browser fullscreen uses the correct painted bounds. Logical fullscreen flags alone cannot close the page-requested action; changing compositor geometry to conceal this sequence is not an accepted repair.

One notification-lifecycle replay lost the application accessibility root after Allow (an actual NoReply response). Separate subsequent basic and effects diagnostics completed all seven checks, including native revocation, with captured bus-owner continuity and unchanged deadlines. These observations are separate from the accepted four-workflow tranche and do not establish a repair for intermittent provider availability. Permission-provider state alone cannot prove a native revoke action.

The optional `--workflow overwrite --portal-tools <immutable-tools-json>` corpus
uses the same standard FileChooser frontend and GTK backend as the office
fixture. It records unique bus owners, provider processes, mapped libraries,
FileChooser version and actual SaveFile requests. Provider startup preserves the
browser's original presentation environment. All providers and files remain in
the private application boundary.

The corpus captures the existing target before each decision, uses the genuine
chooser's measured painted Save button and a physical confirmation shortcut,
and checks Cancel preserves its hash and returns focus. Explicit Replace must
produce the exact target and a painted completed download record. This does not
claim every keyboard-only chooser route, error or disconnected-volume workflow.
The retained browser's direct GTK fallback has a separately retained unsafe
existing-target diagnostic; it is not accepted by this corpus.

`--workflow keyboard-save --portal-tools <immutable-tools-json>` adds a separate
keyboard route. After the disclosed common startup/focus checks, the runner
forbids pointer actions and records each physical keyboard request. It checks
Save As cancellation, a new exact target, existing-target cancellation and
explicit Replace through the captured standard chooser. The same parent,
provider, filesystem hash and completed-download checks remain mandatory. This
route does not certify the unsafe direct GTK fallback or every chooser shortcut.
Chooser focus is observed through the private accessibility provider; real Tab
events traverse to the captured Save control, then Space activates it. The
observer never invokes accessible focus or action methods. Neither an assumed
mnemonic nor a logical default-button flag can substitute for that focus proof.

The fixture build also reads hash-pinned upstream chooser sources and the pinned
GTK header. A narrow parser checks the actual accept-response contract:
Chromium's custom response zero is outside GTK's stock accept responses, so GTK
does not run its ordinary overwrite-validation gate for that response. The
browser deliberately avoids the stock default-button behavior to prevent
held-Enter acceptance. This is a regression diagnostic for the known mismatch,
not a repair; replacing the response value without testing that security
behavior is insufficient. Source changes fail this diagnostic for reassessment.

## Provider source-build feasibility

The pinned package is Brave 1.96.61, distributed by the current package set as
an upstream binary archive. Its public `brave-core` release tag resolves to
`eb024dd75e048e2661c9e1ed29952987ee73b12a`; its release manifest selects Chromium
154.0.8037.98. Source repair is feasible work, rather than an inherent external
block, but changing this binary package's patches cannot rebuild its browser
source. It needs a separately reproducible source-build derivation and then the
same retained-feature and sandbox acceptance.

The [release manifest](https://github.com/brave/brave-core/blob/v1.96.61/package.json)
requires Node 24 and pnpm 11, selects the Chromium revision and supplies locked
JavaScript dependencies. The [release build instructions](https://github.com/brave/brave-core/blob/v1.96.61/README.md)
initialize Chromium plus its dependencies. Chromium additionally uses its pinned
Clang/libc++, GN/Ninja, Python tooling, sysroot and DEPS/CIPD inputs. A reproducible
recipe must also freeze depot_tools, downloaded toolchains, build arguments,
source patches and all initialization outputs; the top-level browser tag alone
does not establish that closure.

The [upstream Linux instructions](https://chromium.googlesource.com/chromium/src/+/main/docs/linux/build_instructions.md)
specify at least 100 GB free disk and recommend more than 16 GB RAM. They warn
that checkout alone can take 30 minutes on a fast connection. Full clean compile
time on this host has not been measured; several hours is a planning estimate,
not an observed result. The inspected host has 16 logical processors and 30 GiB
RAM, with roughly 106 GiB free on each of two separate filesystems. That narrowly
meets the minimum, leaving little margin for retained Nix outputs, source caches
and parallel builds. Before initialization, reserve persistent workspace and
temporary/output capacity explicitly. No full source checkout, toolchain
download or browser compilation is implied by these diagnostic tests.

An upstream repair is available as Chromium commit
[`35f55adbc49923b7508404d9a9a1fb7bc407a600`](https://github.com/chromium/chromium/commit/35f55adbc49923b7508404d9a9a1fb7bc407a600),
landed 2026-10-14. It restores stock accept responses, defers showing the chooser
until Enter/Space activation keys are released, and rejects acceptance during
the first 500 ms while allowing cancellation immediately. Companion tests
cover held activation keys, the acceptance cooldown, immediate cancellation,
destruction and cancellation-mode cleanup. The seven-file change includes
initial tracking setup, exported declarations, generated GTK signatures and test
build dependencies; copying only the response constant omits that safety work.

The inspected official release metadata lists release 1.98.62 with Chromium
156.0.8078.74, the not-yet-promoted release 1.98.63 with 156.0.8078.80, and
nightly 1.101.7 with 157.0.8103.30. Exact chooser sources for 156.0.8078.80 and
157.0.8103.30 still contain the incompatible custom response. A higher version
number alone is therefore not evidence of this repair in the retained browser.
No alternate browser channel or host update is selected by the test corpus.

The pinned package set already has a Chromium source build: fixed dependency
snapshots, depot_tools, GN and system-library/toolchain integration can be reused.
Its source manifest selects 154.0.8037.97, rather than the retained provider's
154.0.8037.98. A retained-provider derivation still needs an exact regenerated
dependency closure and its release-specific customization layer. A small cached
source-subset review found the upstream implementation/header changes apply to
154.0.8037.98; build-file and generated-signature context need adaptation. That
source application check is not compilation or acceptance of the backport.

# Owned content-boundary acceptance

`tests/desktop/content_clipping_native.py` inserts a vivid scrolling grid into
System Settings' actual content parent, underneath its unchanged controls. It
reads the production mask radius and whether that layer is active. A decorative
background radius alone is not interpreted as a content-clipping contract.
Native decoration intentionally retains square client edges; its outer frame
is checked against the nested compositor's declared frame configuration.

The matrix covers basic/effects compositors, native/unified headers and actual
Qt software/OpenGL content. Backend identity and mask activation are asserted,
so an unsupported rendering path cannot silently replace the requested one.
Each run keeps one window mapped while using 1, 1.25, 1.5 and 2 output scales,
790×520 and 520×360 content sizes, and both scroll extremes: sixteen captures.
Expected vivid pixels at all four edges must paint before the boundary scan.
Opaque sentinel pixels outside the declared boundary fail, with at most one
physical pixel of raster tolerance. Absolute physical origins use the scene
renderer's rounding before the local shape calculation.

Real pointer/keyboard input exercises the production search field, production
scroll viewport where it overflows, and the close control at the smallest 2x
window. The grid's exact clipping positions are set separately; its fixture
content does not replace the production input targets. All preference writes
are recorded by a thin private adapter, and no host service is queried.

Build `.#shell`, `.#compositor-basic`, `.#compositor` and `.#test-pointer`. For each
combination, run serially in a new artifact directory:

```sh
bash tests/ci/run-native.sh content_clipping_native nix develop --command \
  desktop-test-python tests/desktop/content_clipping_native.py \
  --launcher /absolute/shell/bin/lumen-desktop-shell \
  --sway /absolute/compositor/bin/sway \
  --pointer-tool /absolute/pointer/bin/wlrctl \
  --native-frame off --content-backend opengl \
  --output /absolute/new-artifact-directory
```

Use `--native-frame on` for native decoration, `--content-backend software` for
Qt's software path, and `--effects` with the effects compositor. Private HOME,
XDG directories, activation-free bus, output settings and virtual input are
created and destroyed by the fixture. Source/assets/helper paths and hashes,
native geometry, backend state, captures and input outcomes are retained.

The persistent pointer helper's `w N` command emits actual wheel-axis events:
its protocol discrete value is in notches, which wlroots converts to 120-unit
steps. This avoids the compositor command interface's legacy one-unit wheel
value. Existing motion/press/release commands retain their protocol.

This is a representative owned-frame acceptance test. It does not claim every
application interior or a measured external reference shape is covered.

## Client fit contract

System Settings, Calendar and Files use the shared `WindowGeometry` component
and pinned `lumen-window-geometry` helper. Consumer margins are subtracted from
the authoritative native workspace dimensions; these are outer fit budgets,
not client dimensions. An explicit outer-budget API remains available. The helper measures container/client rectangles
and titlebar padding from the native tree, then subtracts that padding before
clamping the current client size. It intersects the requested budget with the
actual usable workspace and retains each consumer's existing minimum client
size. If that minimum cannot satisfy the presentation margin, the result says
so; if it cannot fit the usable workspace, fitting is unsupported. Tiled,
fullscreen, paired, filled, minimized and owned/modal placements are preserved.

Fitting never enlarges a client. An already-fitting in-bounds window is a true
no-op, preserving explicit position. Only an out-of-bounds prospective outer
frame is recentered within the usable workspace. Screen identity changes also
schedule fitting, even when the destination output already exists and emits no
new size signal. Requests coalesce and execute serially; closing the owning
window cancels pending work and its observer. An acknowledged native window/output
subscription re-samples after ACK and tracks the exact captured owner and
workspace identity/rectangle. This handles oversized windows spanning two outputs,
where the Qt screen pointer can remain on the original display. Same-workspace
move/resize events do not trigger fits; socket/process epochs and retired native
containers cannot be retargeted. Title/container changes retire and rebuild the
observer before draining the new request.

A direct child process establishes the owning UI PID. App ID, literal title,
container ID, process start and compositor socket identity are checked before
dispatch. Compound native criteria enforce the captured target again. Criteria
use absolute PCRE2 UTF code-point literals, so names containing quotes,
backslashes, command punctuation, non-ASCII characters or newlines remain
literal. An initially mapping window uses an acknowledged, bounded native map
event wait; an explicit retired container is rejected immediately.

The native fixture additionally oversizes the client, checks measured outer
budget clamping, repeats five coalesced fit calls without shrinking/recentering,
moves the same mapped window to an already-existing smaller fractional output
and back without enlargement, rejects duplicate same-PID/title candidates and a
retired container, and fits a literal punctuation/Unicode title. Protocol units
exercise initial mapping, stale socket/process identity and unsupported layout
states. This test found the former native client/outer-size mismatch; the
shared helper replaces those raw resize requests.


## Consumer glue acceptance

`consumer_fit_native.py` opens the actual Calendar and Files windows in a private
fragment. It preserves the large initial Calendar size that spans both outputs
on moving to the smaller display; no pre-shrink conceals screen-selection
ambiguity. Each consumer must fit its own outer margin, survive five coalesced
no-op requests, and return without enlargement. Files retains a real selected
synthetic text file and its rendered preview. Small-output captures are retained.
The copied Files backend only replaces unused volume discovery with an empty
private list; original/copied hashes record that adapter. All file contents,
preferences and preview leases are private. The calendar executable accepts
read-only requests and returns an empty synthetic provider catalogue; its hash
is retained. There is no session activation directory or host system bus.

```sh
bash tests/ci/run-native.sh consumer_fit_native \
  nix develop --command desktop-test-python tests/desktop/consumer_fit_native.py \
  --launcher /absolute/package/bin/lumen-desktop-shell \
  --sway /absolute/compositor/bin/sway --native-frame on \
  --output /absolute/new-consumer-artifact-directory
```

CI runs both frame modes with basic/effects compositors. This bounded two-window
screen-change proof complements the Settings clipping corpus; it does not claim
all internal Calendar/Files interactions or provider integrations.

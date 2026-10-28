# Mixed output scaling acceptance

`lumen-mixed-output-scale-fixture` (Rust 2024) runs in a disposable HOME/XDG tree,
activation-free private session bus and two nested compositor outputs. It does
not change the login session or physical display settings.

Build `.#shell`, `.#compositor-basic`, `.#compositor`, `.#test-pointer` and
`.#test-display-visual-fixtures`,
then run each compositor serially:

```sh
nix develop --command nu tests/ci/run-native.nu mixed_output_scale_native -- \
  /absolute/display-visual-fixtures/bin/lumen-mixed-output-scale-fixture \
  --launcher /absolute/shell/bin/lumen-desktop-shell \
  --sway /absolute/compositor/bin/sway \
  --pointer-tool /absolute/pointer/bin/wlrctl \
  --source res/lumen-desktop \
  --output /absolute/new-artifact-directory
```

Add `--effects` for the effects compositor. This uses genuine software GLES
SceneFX rendering, including its framebuffers, rather than replacing the
compositor renderer with a basic one. Window content uses Qt's software backend.

The same native-framed specimen and the production Help window each retain their
container identity through ten output/mode transitions at 1, 1.25, 1.5 and 2.
The fixture records compositor logical rectangles separately from physical
captures. Raster extent rounding is limited to one physical pixel. Content
size, text/icon/control logical extents and reachability remain unchanged.
Painted body/text/icon pixels and native control colours must appear before
input proceeds. Every scale exercises a positive content action; native green
controls enter fullscreen and restore their original geometry. Owned close
controls also exercise held-pointer cancellation and a positive close.

`result.json` records source revision, fixture and copied asset hashes, package
paths, native geometry, physical sizes, edge/corner samples and action outcomes.
Failures preserve the last completed state, current capture, diagnostic state
and child process return codes. Corners are sampled without prescribing one
compositor's shape to another. These are engineering invariants, not measured
reference geometry or a hardware sharpness/blur comparison.

The native allocator patch aligns udmabuf rows to cache lines and pads backing
storage to four raster rows while keeping requested buffer dimensions. Without
this, a 30-pixel cursor at 1.25 scale has a 120-byte row stride: software SIMD
rasterization can issue an aligned load from an unaligned following row and
crash. The mixed-output fixture retains that real cursor path throughout; it
does not hide the fault by disabling effects or changing cursor sizes.

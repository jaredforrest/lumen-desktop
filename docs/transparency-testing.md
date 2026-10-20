# Live transparency fallback

Global reduced transparency is a minimum opacity policy. Calendar and
notification cards combine it with their local setting and increased contrast;
a local false value cannot undo a global accessibility preference. Existing
opaque surfaces stay opaque. The material substitution does not change layout,
window ownership, hit regions or captured input targets.

`tests/desktop/live_transparency_native.py` keeps production Remote Desktop,
Help and Session windows, a PanelSurface control popover, compact calendar,
notification card and Dock mapped through normal/opaque/normal atomic appearance
updates in both light and dark palettes. It holds a real pointer press over the
same control during each update and checks the exact release action. Text-entry,
button and Dock keyboard routes are also exercised after the updates.

The fixture compares native window identity and compositor rectangles, item and
text rectangles, focus, and the Dock's mask, reserved area and model. Floating
window coordinates come from compositor IPC. Layer coordinates come from the
actual output rectangle and published layer anchors/margins, validated with
painted pixels and pointer actions. It does not infer global client positions
from Wayland `mapToGlobal`.

A private patterned background reveals residual translucency. A bounded paint
check compares captured RGB pixels against the declared material and underlying
pattern, then requires opaque fallback when reduction is enabled. Primary foreground
pixels must actually be painted and that foreground must meet 4.5:1 contrast
against the sampled material. The last mismatch and captures remain available on failure.
The renderer uses Qt software drawing and real basic/effects native compositors;
this is not a measurement of hardware blur quality or a reference-system claim.

Build `.#shell`, `.#compositor-basic`, `.#compositor` and `.#test-pointer`, then run:

```sh
nix develop --command desktop-test-python tests/desktop/live_transparency_native.py \
  --launcher /path/to/shell/bin/lumen-desktop-shell \
  --sway /path/to/compositor/bin/sway \
  --pointer-tool /path/to/pointer/bin/wlrctl --output /new/output/directory
```

Add `--effects` for the effects compositor. CI runs both variants and retains
`artifacts/live-transparency-*`. All provider actions are private recorded
intents; no remote connection, power action, account or host compositor is used.

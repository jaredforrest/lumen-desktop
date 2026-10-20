# Icon catalogue acceptance

Run the standalone entry, which creates its own disposable HOME, XDG state,
activation-free session bus and inert system/audio addresses:

```sh
bash tests/ci/run-native.sh icon_render \
  nix develop --command desktop-test-python tests/desktop/icon_render.py \
  --assets "$PWD/res/lumen-desktop/shell" \
  --launcher /absolute/package/bin/lumen-desktop-shell \
  --sway /absolute/compositor/bin/sway --output /absolute/new-artifact-directory
```

The fixture retains the actual Dock and launcher captures, using three synthetic
application registrations. It also paints every public system/application SVG
through `AppIcon`, including dynamic calendar labels and supported dark variants,
and every named production `SystemGlyph`. Sizes 16/24/64 appear at output scales
1, 1.25, 1.5 and 2, in both themes and all three supported icon preference values.
Every sample must have actual painted pixels, fit its logical cell, and retain
sufficient source raster size. The logical client geometry stays fixed; native
screenshots record physical output pixels. The declared variant must match the
actual resolved source and painted source colour, with no missing-asset fallback
or QML binding errors. Background and semantic-ink pixels must match the live
profile, so a logically updated preference cannot accept an old presented frame.

Raw system SVGs use labelled neutral contrast plates because consumers normally
tint them or use the software symbol painter. Those plates are fixture content;
the separate symbol catalogue uses the actual semantic foreground/background.
The fixture uses the software content renderer and basic nested compositor. It
provides reproducible raster/layout evidence, not an external optical reference
comparison, a hardware renderer certification or a redesign of the icons.

The copied full-shell provider seams use existing unavailable-provider stubs and
record source/helper hashes. Application data, preferences and search roots are
private; no host provider actions or live input are used. Public artwork and
source fonts retain their existing ownership boundary. No official or private
consumer artwork is added to the repository.

`test_icon_render_bootstrap.py` launches the actual outer Python entry through a
recording child transport and validates its environment, bus configuration and
cleanup. This catches an outer-entry failure even when an inner-only invocation
would bypass it. CI also runs the genuine standalone render and retains its
captures and per-sample pixel results on failure.

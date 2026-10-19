# Desktop widget visibility and material

Desktop widgets retain the existing Automatic, Full Colour and Monochrome modes.
Automatic uses the production native focus bridge: an active application makes
widgets monochrome and deemphasises them; focusing a widget or desktop restores
colour. Full Colour and Monochrome remain explicit overrides. The current dimming
amount is a product choice, not a measured reference value. Neither colouring nor
dimming disables widget controls.

The visibility preference releases the widget input surface when hidden and keeps
existing widget identities and manual calendar selection when shown again. Each
widget's existing display selector determines which connected output presents it;
it does not change host output configuration. Default grid positions may reflow
when the set of widgets on an output changes.

Widget-specific Reduce Transparency now applies to the outer material of clock
and calendar widgets as well as inner calendar content. Shared Reduce Transparency
and Increase Contrast also apply. Opaque mode disables opacity-based dimming;
monochrome styling remains. This avoids reintroducing transparency through an
ancestor's opacity. Monochrome selected-day foregrounds retain readable contrast
in both palettes.

Compact calendar typography honors the shared text-size preference. Month and
day controls grow with their labels; the related-app row uses its natural height.
Constrained widget sizes scroll, and keyboard focus reveals their controls. Clock
text retains bounded fitting inside the supported sizes. This is owned-widget
coverage, not a claim about third-party widgets or reference-exact metrics.

The production focus bridge now supplies the explicit priority argument to
`GLib.io_add_watch`. This preserves existing callback semantics while removing
a deprecation that otherwise aborts real focus monitoring under strict warnings.

## Acceptance

```
nix flake check --print-build-logs --option abort-on-warn true
nix develop --command desktop-test-python tests/desktop/widget_visibility_native.py \
  --launcher "$(nix build .#shell --no-link --print-out-paths)/bin/lumen-desktop-shell" \
  --sway "$(nix build .#compositor-basic --no-link --print-out-paths)/bin/sway" \
  --pointer-tool "$(nix build .#test-pointer --no-link --print-out-paths)/bin/wlrctl" \
  --output /tmp/widget-visibility-acceptance
```

CI also runs the effects variant with `--effects`. The isolated fixture uses two
native outputs, the actual production focus bridge and a disposable external app.
It verifies automatic/explicit colouring with painted pixels, actual focus and
input, hidden input-mask release, scoped display routing, opaque material pixels,
and usable calendar/clock controls at 150% text in all three supported sizes.
Calendar data, widget settings, HOME and the session bus are disposable. No live
accounts, private host configuration, output policy or desktop session is changed.

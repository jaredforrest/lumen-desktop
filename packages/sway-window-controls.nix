{ pkgs, effects ? false }:
let
  compositor = if effects then pkgs.swayfx-unwrapped else pkgs.sway-unwrapped;
  expectedVersion = if effects then "0.6" else "1.12";
  framePatch = if effects then ../res/lumen-desktop/swayfx-traffic-lights.patch else ../res/lumen-desktop/sway-traffic-lights.patch;
in
assert pkgs.lib.assertMsg (compositor.version == expectedVersion)
  "Revalidate the shared window-frame patch before upgrading Sway.";
pkgs.sway.override {
  sway-unwrapped = compositor.overrideAttrs (old: {
    patches = (old.patches or [ ]) ++ [ framePatch ]
      ++ pkgs.lib.optional effects ../res/lumen-desktop/swayfx-edge-layout-compat.patch
      ++ [ ../res/lumen-desktop/sway-hot-corners.patch ../res/lumen-desktop/sway-edge-layouts.patch ../res/lumen-desktop/sway-file-drag.patch
           ../res/lumen-desktop/sway-paired-layouts.patch ../res/lumen-desktop/sway-window-relationships.patch
           ../res/lumen-desktop/sway-desktop-lifecycle.patch ../res/lumen-desktop/sway-session-lock-state.patch
           ../res/lumen-desktop/sway-window-groups.patch ];
  });
}

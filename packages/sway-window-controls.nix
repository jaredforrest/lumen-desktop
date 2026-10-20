{ pkgs, effects ? false }:
let
  alignedWlroots = pkgs.wlroots_0_20.overrideAttrs (old: {
    patches = (old.patches or [ ]) ++ [ ../res/lumen-desktop/wlroots-udmabuf-raster-alignment.patch ../res/lumen-desktop/wlroots-xwayland-modal-state.patch ];
  });
  compositor = if effects then pkgs.swayfx-unwrapped.override {
    wlroots_0_20 = alignedWlroots;
    scenefx = pkgs.scenefx.override { wlroots_0_20 = alignedWlroots; };
  } else pkgs.sway-unwrapped.override { wlroots_0_20 = alignedWlroots; };
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
           ../res/lumen-desktop/sway-window-groups.patch ../res/lumen-desktop/sway-group-selector.patch ../res/lumen-desktop/sway-group-membership.patch
           ../res/lumen-desktop/sway-layer-input-rebase.patch ../res/lumen-desktop/sway-xwayland-relationships.patch ../res/lumen-desktop/sway-preferred-zoom.patch ../res/lumen-desktop/sway-placement-restore-commit.patch ];
  });
}

{ pkgs, effects ? false, displaySpaces ? true, ownedGeometryEvents ? true, nativeFocusGuard ? true, allDesktopProjection ? (nativeFocusGuard && displaySpaces && ownedGeometryEvents) }:
let
  alignedWlroots = pkgs.wlroots_0_20.overrideAttrs (old: {
    patches = (old.patches or [ ]) ++ [ ../res/lumen-desktop/wlroots-udmabuf-raster-alignment.patch ../res/lumen-desktop/wlroots-xwayland-modal-state.patch ../res/lumen-desktop/wlroots-seat-capability-leave.patch ];
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
           ../res/lumen-desktop/sway-paired-layouts.patch ../res/lumen-desktop/sway-paired-scratchpad-preflight.patch ../res/lumen-desktop/sway-window-relationships.patch
           ../res/lumen-desktop/sway-desktop-lifecycle.patch ../res/lumen-desktop/sway-session-lock-state.patch
           ../res/lumen-desktop/sway-window-groups.patch ../res/lumen-desktop/sway-group-selector.patch ../res/lumen-desktop/sway-group-membership.patch
           ../res/lumen-desktop/sway-layer-input-rebase.patch ../res/lumen-desktop/sway-xwayland-relationships.patch
           ../res/lumen-desktop/sway-preferred-zoom.patch ../res/lumen-desktop/sway-placement-restore-commit.patch ]
      ++ pkgs.lib.optional displaySpaces ../res/lumen-desktop/sway-display-spaces.patch
      ++ [ ../res/lumen-desktop/sway-disabled-action-dispatch.patch ]
      ++ pkgs.lib.optional ownedGeometryEvents ../res/lumen-desktop/sway-owned-geometry-events.patch
      ++ [ ../res/lumen-desktop/sway-interactive-resize-sync.patch ../res/lumen-desktop/sway-shortcut-load-state.patch ]
      ++ [ ../res/lumen-desktop/sway-toplevel-identity.patch ../res/lumen-desktop/sway-shortcut-layout-observation.patch ];
    postPatch = (old.postPatch or "") + pkgs.lib.optionalString nativeFocusGuard ''
      ${pkgs.patch}/bin/patch --batch --fuzz=0 -p1 < ${if effects then ../res/lumen-desktop/swayfx-native-focus-guard.patch else ../res/lumen-desktop/sway-native-focus-guard.patch}
      ${pkgs.patch}/bin/patch --batch --fuzz=0 -p1 < ${../res/lumen-desktop/sway-native-focus-guard-core.patch}
    '' + pkgs.lib.optionalString allDesktopProjection ''
      ${pkgs.patch}/bin/patch --batch --fuzz=0 -p1 < ${../res/lumen-desktop/sway-all-desktop-projection.patch}
      ${pkgs.patch}/bin/patch --batch --fuzz=0 -p1 < ${if effects then ../res/lumen-desktop/swayfx-all-desktop-projection-scene.patch else ../res/lumen-desktop/sway-all-desktop-projection-scene.patch}
    '' + pkgs.lib.optionalString (nativeFocusGuard && displaySpaces) ''
      ${pkgs.patch}/bin/patch --batch --fuzz=0 -p1 < ${../res/lumen-desktop/sway-space-transfer.patch}
      ${pkgs.stdenv.cc}/bin/cc -std=c11 -Wall -Wextra -Werror -pedantic -Iinclude -I${pkgs.json_c.dev}/include/json-c test/lumen_transfer_policy_test.c -L${pkgs.json_c}/lib -Wl,-rpath,${pkgs.json_c}/lib -ljson-c -o .lumen-transfer-boundary-test
      ./.lumen-transfer-boundary-test
    '' + ''
      ${pkgs.patch}/bin/patch --batch --fuzz=0 -p1 < ${../res/lumen-desktop/sway-session-lock-input-focus.patch}
    '';
    mesonFlags = (old.mesonFlags or [ ]) ++ [ "-Dwerror=true" ];
    buildInputs = (old.buildInputs or [ ]) ++ pkgs.lib.optional nativeFocusGuard pkgs.openssl;
  });
}

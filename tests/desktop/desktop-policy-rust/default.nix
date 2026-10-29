{ pkgs }:
pkgs.rustPlatform.buildRustPackage {
  pname = "lumen-desktop-policy-fixture";
  version = "0.1.0";
  src = ./.;
  cargoLock.lockFile = ./Cargo.lock;
  env.LUMEN_DBUS_DAEMON = "${pkgs.dbus}/bin/dbus-daemon";
  env.LUMEN_GRIM = "${pkgs.grim}/bin/grim";
  postInstall = ''
    mkdir -p "$out/share/lumen-desktop-policy-fixture"
    cp application.qml lifecycle.qml display_recovery.qml owned_geometry.qml owned_components.qml owned_inactive.qml typography.qml desktop_typography.qml utility_typography.qml calendar_detail_typography.qml overview_typography.qml folder_stack.qml launcher_typography.qml shared_control_typography.qml dock_motion.qml reduced_motion.qml semantic_state.qml live_overlay.qml pointer_retirement.qml window_controls_client.qml window_controls_dock.qml minimized_dock.qml dock_offer_peer.qml dock_offer_dock.qml "$out/share/lumen-desktop-policy-fixture/"
  '';
}

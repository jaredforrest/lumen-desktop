{ pkgs }:
(import ../../../packages/owned-rust.nix { inherit pkgs; }) {
  pname = "lumen-window-layout-menu-fixture";
  version = "0.1.0";
  src = ./.;
  cargoLock.lockFile = ./Cargo.lock;
  env.LUMEN_TEST_DBUS_DAEMON = "${pkgs.dbus}/bin/dbus-daemon";
  env.LUMEN_TEST_MESA = "${pkgs.mesa}";
  postInstall = ''
    mkdir -p "$out/share/lumen-window-layout-menu-fixture"
    cp fixture.qml "$out/share/lumen-window-layout-menu-fixture/fixture.qml"
    cp corner-preview.qml "$out/share/lumen-window-layout-menu-fixture/corner-preview.qml"
  '';
}

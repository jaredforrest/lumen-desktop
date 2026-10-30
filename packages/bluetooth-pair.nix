{ pkgs }:
(import ./owned-rust.nix { inherit pkgs; }) {
  pname = "lumen-bluetooth-pair";
  version = "0.1.0";
  src = ../res/lumen-desktop/bluetooth-pair;
  cargoLock.lockFile = ../res/lumen-desktop/bluetooth-pair/Cargo.lock;
  nativeCheckInputs = [ pkgs.dbus pkgs.coreutils ];
  LUMEN_TEST_DBUS_CONFIG = "${pkgs.dbus}/share/dbus-1/session.conf";
}

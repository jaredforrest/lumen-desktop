{ pkgs }:
pkgs.rustPlatform.buildRustPackage {
  pname = "lumen-force-quit-native";
  version = "0.1.0";
  src = ./.;
  cargoLock.lockFile = ./Cargo.lock;
  LUMEN_DBUS_RUN_SESSION = "${pkgs.dbus}/bin/dbus-run-session";
}

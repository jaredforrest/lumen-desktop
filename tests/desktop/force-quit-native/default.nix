{ pkgs }:
pkgs.rustPlatform.buildRustPackage {
  pname = "lumen-force-quit-native";
  version = "0.1.0";
  src = ./.;
  cargoLock.lockFile = ./Cargo.lock;
  nativeBuildInputs = [ pkgs.pkg-config ];
  buildInputs = [ pkgs.gtk3 ];
  LUMEN_DBUS_RUN_SESSION = "${pkgs.dbus}/bin/dbus-run-session";
}

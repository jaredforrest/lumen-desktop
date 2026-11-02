{ pkgs, privateBus }:
(import ../../../packages/owned-rust.nix { inherit pkgs; }) {
  pname = "lumen-force-quit-native";
  version = "0.1.0";
  src = ./.;
  cargoLock.lockFile = ./Cargo.lock;
  nativeBuildInputs = [ pkgs.pkg-config ];
  buildInputs = [ pkgs.gtk3 ];
  LUMEN_DBUS_RUN_SESSION = "${privateBus}/bin/dbus-run-session";
  env.LUMEN_TEST_DBUS_SEND = "${pkgs.dbus}/bin/dbus-send";
  env.LUMEN_TEST_TIMEOUT = "${pkgs.coreutils}/bin/timeout";
}

{ pkgs }:
let
  binary = (import ./owned-rust.nix { inherit pkgs; }) {
    pname = "lumen-privacy-status";
    version = "0.1.0";
    src = ../res/lumen-desktop/privacy-status;
    cargoLock.lockFile = ../res/lumen-desktop/privacy-status/Cargo.lock;
    nativeCheckInputs = [ pkgs.dbus pkgs.coreutils ];
    LUMEN_PRIVACY_TEST_DBUS_CONFIG = "${pkgs.dbus}/share/dbus-1/session.conf";
  };
in (import ./nu-wrapped-rust.nix { inherit pkgs; }) {
  name = "lumen-privacy-status";
  inherit binary;
  runtimeInputs = [ pkgs.coreutils pkgs.util-linux ];
}

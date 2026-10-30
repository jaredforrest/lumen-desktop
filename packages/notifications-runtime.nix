{ pkgs }:
(import ./owned-rust.nix { inherit pkgs; }) {
  pname = "lumen-notifications-runtime";
  version = "0.1.0";
  src = ../res/lumen-desktop/notifications/runtime;
  cargoLock.lockFile = ../res/lumen-desktop/notifications/runtime/Cargo.lock;
  nativeCheckInputs = [ pkgs.dbus ];
}

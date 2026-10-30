{ pkgs }:
(import ./owned-rust.nix { inherit pkgs; }) {
  pname = "lumen-input-settings";
  version = "0.1.0";
  src = ../res/lumen-desktop/input-settings;
  cargoLock.lockFile = ../res/lumen-desktop/input-settings/Cargo.lock;
}

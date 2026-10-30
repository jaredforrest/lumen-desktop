{ pkgs }:
(import ./owned-rust.nix { inherit pkgs; }) {
  pname = "lumen-display-settings";
  version = "0.1.0";
  src = ../res/lumen-desktop/display-settings;
  cargoLock.lockFile = ../res/lumen-desktop/display-settings/Cargo.lock;
}

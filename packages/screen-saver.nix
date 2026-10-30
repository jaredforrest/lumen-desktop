{ pkgs }:
(import ./owned-rust.nix { inherit pkgs; }) {
  pname = "lumen-screen-saver";
  version = "0.1.0";
  src = ../res/lumen-desktop/screen-saver;
  cargoLock.lockFile = ../res/lumen-desktop/screen-saver/Cargo.lock;
}

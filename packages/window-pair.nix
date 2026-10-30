{ pkgs }:
(import ./owned-rust.nix { inherit pkgs; }) {
  pname = "lumen-window-pair";
  version = "0.1.0";
  src = ../res/lumen-desktop/window-pair;
  cargoLock.lockFile = ../res/lumen-desktop/window-pair/Cargo.lock;
}

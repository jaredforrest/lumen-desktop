{ pkgs }:
(import ./owned-rust.nix { inherit pkgs; }) {
  pname = "lumen-window-geometry";
  version = "0.1.0";
  src = ../res/lumen-desktop/window-geometry;
  cargoLock.lockFile = ../res/lumen-desktop/window-geometry/Cargo.lock;
}

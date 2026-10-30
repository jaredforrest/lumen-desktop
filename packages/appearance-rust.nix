{ pkgs }:
(import ./owned-rust.nix { inherit pkgs; }) {
  pname = "lumen-appearance";
  version = "0.1.0";
  src = ../res/lumen-desktop/appearance;
  cargoRoot = "runtime";
  buildAndTestSubdir = "runtime";
  cargoLock.lockFile = ../res/lumen-desktop/appearance/runtime/Cargo.lock;
}

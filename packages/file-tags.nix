{ pkgs }:
(import ./owned-rust.nix { inherit pkgs; }) {
  pname = "lumen-file-tags";
  version = "0.1.0";
  src = ../res/lumen-desktop/file-tags;
  cargoLock.lockFile = ../res/lumen-desktop/file-tags/Cargo.lock;
}

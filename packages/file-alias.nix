{ pkgs }:
(import ./owned-rust.nix { inherit pkgs; }) {
  pname = "lumen-file-alias";
  version = "0.1.0";
  src = ../res/lumen-desktop/file-alias;
  cargoLock.lockFile = ../res/lumen-desktop/file-alias/Cargo.lock;
}

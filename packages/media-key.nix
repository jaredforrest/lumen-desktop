{ pkgs }:
(import ./owned-rust.nix { inherit pkgs; }) {
  pname = "lumen-media-key";
  version = "0.1.0";
  src = ../res/lumen-desktop/media-key;
  cargoLock.lockFile = ../res/lumen-desktop/media-key/Cargo.lock;
}

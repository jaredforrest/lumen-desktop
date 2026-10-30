{ pkgs }:
(import ./owned-rust.nix { inherit pkgs; }) {
  pname = "lumen-spaces";
  version = "0.1.0";
  src = ../res/lumen-desktop/spaces;
  cargoLock.lockFile = ../res/lumen-desktop/spaces/Cargo.lock;
}

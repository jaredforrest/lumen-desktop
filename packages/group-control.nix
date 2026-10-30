{ pkgs }:
(import ./owned-rust.nix { inherit pkgs; }) {
  pname = "lumen-group-control";
  version = "0.1.0";
  src = ../res/lumen-desktop/group-control;
  cargoLock.lockFile = ../res/lumen-desktop/group-control/Cargo.lock;
}

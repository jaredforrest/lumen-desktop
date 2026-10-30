{ pkgs }:
(import ./owned-rust.nix { inherit pkgs; }) {
  pname = "lumen-capture-observation";
  version = "0.1.0";
  src = ../res/lumen-desktop/capture-observation;
  cargoLock.lockFile = ../res/lumen-desktop/capture-observation/Cargo.lock;
}

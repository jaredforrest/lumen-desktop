{ pkgs }:
(import ./owned-rust.nix { inherit pkgs; }) {
  pname = "lumen-desktop-state";
  version = "0.1.0";
  src = ../res/lumen-desktop/desktop-state;
  cargoLock.lockFile = ../res/lumen-desktop/desktop-state/Cargo.lock;
}

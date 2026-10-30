{ pkgs }:
(import ./owned-rust.nix { inherit pkgs; }) {
  pname = "lumen-asar-patch";
  version = "0.1.0";
  src = ../res/lumen-desktop/global-menu/asar-patch;
  cargoLock.lockFile = ../res/lumen-desktop/global-menu/asar-patch/Cargo.lock;
}

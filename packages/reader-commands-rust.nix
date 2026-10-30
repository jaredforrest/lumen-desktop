{ pkgs }:
(import ./owned-rust.nix { inherit pkgs; }) {
  pname = "lumen-reader-commands";
  version = "0.1.0";
  src = ../res/lumen-desktop/reader-commands;
  cargoLock.lockFile = ../res/lumen-desktop/reader-commands/Cargo.lock;
}

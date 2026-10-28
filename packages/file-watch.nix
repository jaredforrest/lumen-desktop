{ pkgs }:
pkgs.rustPlatform.buildRustPackage {
  pname = "lumen-file-watch";
  version = "0.1.0";
  src = ../res/lumen-desktop/file-watch;
  cargoLock.lockFile = ../res/lumen-desktop/file-watch/Cargo.lock;
}

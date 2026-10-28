{ pkgs }:
pkgs.rustPlatform.buildRustPackage {
  pname = "lumen-batch-rename";
  version = "0.1.0";
  src = ../res/lumen-desktop/batch-rename;
  cargoLock.lockFile = ../res/lumen-desktop/batch-rename/Cargo.lock;
}

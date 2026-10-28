{ pkgs }:
pkgs.rustPlatform.buildRustPackage {
  pname = "lumen-shell-ipc";
  version = "0.1.0";
  src = ../res/lumen-desktop/shell-ipc;
  cargoLock.lockFile = ../res/lumen-desktop/shell-ipc/Cargo.lock;
}

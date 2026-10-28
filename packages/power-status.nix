{ pkgs }:
pkgs.rustPlatform.buildRustPackage {
  pname = "lumen-power-status";
  version = "0.1.0";
  src = ../res/lumen-desktop/power-status;
  cargoLock.lockFile = ../res/lumen-desktop/power-status/Cargo.lock;
}

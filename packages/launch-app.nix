{ pkgs }:
pkgs.rustPlatform.buildRustPackage {
  pname = "lumen-launch-app";
  version = "0.1.0";
  src = ../res/lumen-desktop/launch-app;
  cargoLock.lockFile = ../res/lumen-desktop/launch-app/Cargo.lock;
}

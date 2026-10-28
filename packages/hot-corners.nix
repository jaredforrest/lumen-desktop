{ pkgs }:
pkgs.rustPlatform.buildRustPackage {
  pname = "lumen-hot-corners-runtime";
  version = "0.1.0";
  src = ../res/lumen-desktop/hot-corners;
  cargoLock.lockFile = ../res/lumen-desktop/hot-corners/Cargo.lock;
}

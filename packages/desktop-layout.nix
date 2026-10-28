{ pkgs }:
pkgs.rustPlatform.buildRustPackage {
  pname = "lumen-desktop-layout";
  version = "0.1.0";
  src = ../res/lumen-desktop/desktop-layout;
  cargoLock.lockFile = ../res/lumen-desktop/desktop-layout/Cargo.lock;
}

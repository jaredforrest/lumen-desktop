{ pkgs }:
pkgs.rustPlatform.buildRustPackage {
  pname = "lumen-remote-desktop";
  version = "0.1.0";
  src = ../res/lumen-desktop/remote-desktop;
  cargoLock.lockFile = ../res/lumen-desktop/remote-desktop/Cargo.lock;
}

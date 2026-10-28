{ pkgs }:
pkgs.rustPlatform.buildRustPackage {
  pname = "lumen-network-connectivity";
  version = "0.1.0";
  src = ../res/lumen-desktop/network-connectivity;
  cargoLock.lockFile = ../res/lumen-desktop/network-connectivity/Cargo.lock;
}

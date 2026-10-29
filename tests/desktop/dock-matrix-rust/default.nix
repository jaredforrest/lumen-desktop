{ pkgs }:
pkgs.rustPlatform.buildRustPackage {
  pname = "lumen-dock-matrix-fixture";
  version = "0.1.0";
  src = ./.;
  cargoLock.lockFile = ./Cargo.lock;
}

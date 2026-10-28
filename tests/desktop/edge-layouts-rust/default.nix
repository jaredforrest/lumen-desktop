{ pkgs }:
pkgs.rustPlatform.buildRustPackage {
  pname = "lumen-edge-layouts-fixture";
  version = "0.1.0";
  src = ./.;
  cargoLock.lockFile = ./Cargo.lock;
}

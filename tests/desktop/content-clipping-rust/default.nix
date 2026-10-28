{ pkgs }:
pkgs.rustPlatform.buildRustPackage {
  pname = "lumen-content-clipping-fixture";
  version = "0.1.0";
  src = ./.;
  cargoLock.lockFile = ./Cargo.lock;
}

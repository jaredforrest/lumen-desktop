{ pkgs }:
pkgs.rustPlatform.buildRustPackage {
  pname = "lumen-material-hierarchy-fixture";
  version = "0.1.0";
  src = ./.;
  cargoLock.lockFile = ./Cargo.lock;
}

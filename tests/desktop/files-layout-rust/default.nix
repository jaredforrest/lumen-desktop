{ pkgs }:
pkgs.rustPlatform.buildRustPackage {
  pname = "lumen-files-layout-fixture";
  version = "0.1.0";
  src = ./.;
  cargoLock.lockFile = ./Cargo.lock;
}

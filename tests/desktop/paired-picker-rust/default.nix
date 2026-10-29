{ pkgs }:
pkgs.rustPlatform.buildRustPackage {
  pname = "lumen-paired-picker-fixture";
  version = "0.1.0";
  src = ./.;
  cargoLock.lockFile = ./Cargo.lock;
}

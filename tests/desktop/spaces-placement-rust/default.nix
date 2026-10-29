{ pkgs }:
pkgs.rustPlatform.buildRustPackage {
  pname = "lumen-spaces-placement-fixture";
  version = "0.1.0";
  src = ./.;
  cargoLock.lockFile = ./Cargo.lock;
}

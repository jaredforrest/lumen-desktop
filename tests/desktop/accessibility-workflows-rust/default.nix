{ pkgs }:
pkgs.rustPlatform.buildRustPackage {
  pname = "lumen-accessibility-workflows-fixture";
  version = "0.1.0";
  src = ./.;
  cargoLock.lockFile = ./Cargo.lock;
}

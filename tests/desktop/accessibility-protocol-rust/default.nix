{ pkgs }:
pkgs.rustPlatform.buildRustPackage {
  pname = "lumen-accessibility-protocol-fixture";
  version = "0.1.0";
  src = ./.;
  cargoLock.lockFile = ./Cargo.lock;
}

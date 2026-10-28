{ pkgs }:
pkgs.rustPlatform.buildRustPackage {
  pname = "lumen-trash-confirmation-fixture";
  version = "0.1.0";
  src = ./.;
  cargoLock.lockFile = ./Cargo.lock;
}

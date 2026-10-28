{ pkgs }:
pkgs.rustPlatform.buildRustPackage {
  pname = "lumen-office-workflows";
  version = "0.1.0";
  src = ./.;
  cargoLock.lockFile = ./Cargo.lock;
}

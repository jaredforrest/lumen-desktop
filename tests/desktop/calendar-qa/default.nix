{ pkgs }:
pkgs.rustPlatform.buildRustPackage {
  pname = "lumen-calendar-qa";
  version = "0.1.0";
  src = ./.;
  cargoLock.lockFile = ./Cargo.lock;
}

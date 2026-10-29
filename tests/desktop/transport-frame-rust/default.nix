{ pkgs }:
pkgs.rustPlatform.buildRustPackage {
  pname = "lumen-transport-frame-fixture";
  version = "0.1.0";
  src = ./.;
  cargoLock.lockFile = ./Cargo.lock;
}

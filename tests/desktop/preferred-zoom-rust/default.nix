{ pkgs }:
pkgs.rustPlatform.buildRustPackage {
  pname = "lumen-preferred-zoom-fixture";
  version = "0.1.0";
  src = ./.;
  cargoLock.lockFile = ./Cargo.lock;
}

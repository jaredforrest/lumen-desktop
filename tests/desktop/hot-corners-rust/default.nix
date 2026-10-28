{ pkgs }:
pkgs.rustPlatform.buildRustPackage {
  pname = "lumen-hot-corners-fixture";
  version = "0.1.0";
  src = ./.;
  cargoLock.lockFile = ./Cargo.lock;
}

{ pkgs }:
pkgs.rustPlatform.buildRustPackage {
  pname = "lumen-installed-smoke-fixture";
  version = "0.1.0";
  src = ./.;
  cargoLock.lockFile = ./Cargo.lock;
}

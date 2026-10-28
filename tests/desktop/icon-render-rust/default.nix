{ pkgs }:
pkgs.rustPlatform.buildRustPackage {
  pname = "lumen-icon-render-fixture";
  version = "0.1.0";
  src = ./.;
  cargoLock.lockFile = ./Cargo.lock;
}

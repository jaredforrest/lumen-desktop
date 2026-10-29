{ pkgs }:
pkgs.rustPlatform.buildRustPackage {
  pname = "lumen-desktop-surface-fixture";
  version = "0.1.0";
  src = ./.;
  cargoLock.lockFile = ./Cargo.lock;
}

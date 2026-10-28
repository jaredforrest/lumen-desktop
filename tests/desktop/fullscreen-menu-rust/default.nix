{ pkgs }:
pkgs.rustPlatform.buildRustPackage {
  pname = "lumen-fullscreen-menu-fixture";
  version = "0.1.0";
  src = ./.;
  cargoLock.lockFile = ./Cargo.lock;
}
